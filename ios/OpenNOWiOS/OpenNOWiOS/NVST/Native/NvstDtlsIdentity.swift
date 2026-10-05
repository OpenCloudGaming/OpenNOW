import CryptoKit
import Foundation
import OpenSSL

/// The DTLS identity a native NVST bundle announces: a self-signed P-256 certificate and the
/// SHA-256 fingerprint the RTSP ANNOUNCE carries.
///
/// WebRTC authenticates a DTLS peer by hashing its certificate and comparing the result with the
/// fingerprint exchanged out of band — here, the seat's arrives in DESCRIBE and ours goes back in
/// ANNOUNCE. Nothing signs a chain, so a self-signed leaf is what the protocol expects.
public final class NvstDtlsIdentity: @unchecked Sendable {
    public enum IdentityError: LocalizedError, Equatable {
        case keyGenerationFailed
        case certificateCreationFailed
        case certificateSigningFailed
        case encodingFailed
        case contextRejectedCertificate

        public var errorDescription: String? {
            switch self {
            case .keyGenerationFailed: "Could not generate the DTLS private key."
            case .certificateCreationFailed: "Could not build the DTLS certificate."
            case .certificateSigningFailed: "Could not sign the DTLS certificate."
            case .encodingFailed: "Could not encode the DTLS certificate."
            case .contextRejectedCertificate: "The DTLS context rejected the certificate or its key."
            }
        }
    }

    /// Colon-separated uppercase SHA-256 of the DER certificate, the form `general.dtlsFingerprint`
    /// is written in.
    public let fingerprint: String
    /// The DER-encoded certificate, which is what the fingerprint is taken over.
    public let derCertificate: Data

    private let key: OpaquePointer
    private let certificate: OpaquePointer

    /// Builds a fresh identity. Each session announces a new certificate, as the vendor client does.
    public init(commonName: String = "OpenNOW") throws {
        guard let key = Self.makePrivateKey() else { throw IdentityError.keyGenerationFailed }
        guard let certificate = Self.makeCertificate(privateKey: key, commonName: commonName) else {
            EVP_PKEY_free(key)
            throw IdentityError.certificateSigningFailed
        }
        guard let der = Self.derEncoding(of: certificate) else {
            EVP_PKEY_free(key)
            X509_free(certificate)
            throw IdentityError.encodingFailed
        }
        self.key = key
        self.certificate = certificate
        self.derCertificate = der
        self.fingerprint = Self.fingerprint(ofDER: der)
    }

    deinit {
        EVP_PKEY_free(key)
        X509_free(certificate)
    }

    /// Installs the certificate and key on a DTLS context. Throws when OpenSSL rejects either, which
    /// is a wiring fault rather than a peer problem.
    public func apply(to context: OpaquePointer) throws {
        guard SSL_CTX_use_certificate(context, certificate) == 1,
              SSL_CTX_use_PrivateKey(context, key) == 1 else {
            throw IdentityError.contextRejectedCertificate
        }
    }

    /// `EVP_PKEY_Q_keygen` is variadic and cannot be called from Swift, so the P-256 context is
    /// built explicitly instead.
    private static func makePrivateKey() -> OpaquePointer? {
        guard let context = EVP_PKEY_CTX_new_id(EVP_PKEY_EC, nil) else { return nil }
        defer { EVP_PKEY_CTX_free(context) }
        guard EVP_PKEY_keygen_init(context) == 1,
              EVP_PKEY_CTX_set_ec_paramgen_curve_nid(context, NID_X9_62_prime256v1) == 1 else { return nil }
        var key: OpaquePointer?
        guard EVP_PKEY_keygen(context, &key) == 1 else { return nil }
        return key
    }

    private static func makeCertificate(privateKey: OpaquePointer, commonName: String) -> OpaquePointer? {
        guard let certificate = X509_new() else { return nil }
        X509_set_version(certificate, 2)
        ASN1_INTEGER_set(X509_get_serialNumber(certificate), 1)
        X509_gmtime_adj(X509_getm_notBefore(certificate), 0)
        // Long enough to outlive any session the seat will keep alive; nothing renews mid-stream.
        X509_gmtime_adj(X509_getm_notAfter(certificate), 60 * 60 * 24 * 365)
        guard X509_set_pubkey(certificate, privateKey) == 1, let name = X509_get_subject_name(certificate) else {
            X509_free(certificate)
            return nil
        }
        // MBSTRING_ASC's value; OpenSSL exposes it as an arithmetic macro Swift does not import.
        let asciiStringType: Int32 = 4097
        let added = commonName.withCString { pointer in
            pointer.withMemoryRebound(to: UInt8.self, capacity: commonName.utf8.count + 1) { bytes in
                X509_NAME_add_entry_by_txt(name, "CN", asciiStringType, bytes, -1, -1, 0)
            }
        }
        guard added == 1, X509_set_issuer_name(certificate, name) == 1 else {
            X509_free(certificate)
            return nil
        }
        guard X509_sign(certificate, privateKey, EVP_sha256()) > 0 else {
            X509_free(certificate)
            return nil
        }
        return certificate
    }

    private static func derEncoding(of certificate: OpaquePointer) -> Data? {
        let length = i2d_X509(certificate, nil)
        guard length > 0 else { return nil }
        var bytes = [UInt8](repeating: 0, count: Int(length))
        var written: Int32 = 0
        bytes.withUnsafeMutableBufferPointer { buffer in
            var cursor = buffer.baseAddress
            written = i2d_X509(certificate, &cursor)
        }
        guard written == length else { return nil }
        return Data(bytes)
    }

    private static func fingerprint(ofDER der: Data) -> String {
        CryptoKit.SHA256.hash(data: der).map { String(format: "%02X", $0) }.joined(separator: ":")
    }
}
