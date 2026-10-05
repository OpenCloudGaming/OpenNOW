import Foundation
import OpenSSL

/// Chain validation is deliberately bypassed: every DTLS-SRTP peer is self-signed, and identity
/// comes from comparing the certificate's digest with the fingerprint exchanged out of band.
/// Accepting here is what lets `SSL_VERIFY_PEER` still *request* the peer's certificate, so both
/// ends have one to check.
private let opennowAcceptAnyCertificate: @convention(c) (Int32, OpaquePointer?) -> Int32 = { _, _ in 1 }

/// One end of a DTLS 1.2 handshake, driven by datagrams rather than by a socket.
///
/// The handshake is fed received datagrams and returns the datagrams to send, so it can be driven
/// from a test with no network at all. `NvstDtlsTransport` pumps the same state machine over the
/// reserved UDP socket; everything hard to diagnose lives here where it is deterministic.
public final class NvstDtlsHandshake: @unchecked Sendable {
    public enum Role {
        case client
        case server
    }

    public enum HandshakeError: LocalizedError, Equatable {
        case contextUnavailable
        case sslUnavailable
        case srtpProfilesRejected
        case handshakeFailed(String)
        case fingerprintMismatch(expected: String, actual: String)
        case peerPresentedNoCertificate
        case keyingExportFailed
        case notConnected

        public var errorDescription: String? {
            switch self {
            case .contextUnavailable: "Could not create the DTLS context."
            case .sslUnavailable: "Could not create the DTLS session."
            case .srtpProfilesRejected: "The DTLS context rejected the SRTP protection profiles."
            case .handshakeFailed(let reason): "DTLS handshake failed: \(reason)"
            case .fingerprintMismatch(let expected, let actual): "The DTLS peer's certificate fingerprint \(actual) does not match the announced \(expected)."
            case .peerPresentedNoCertificate: "The DTLS peer presented no certificate."
            case .keyingExportFailed: "The DTLS session exported no keying material."
            case .notConnected: "The DTLS session is not connected yet."
            }
        }
    }

    /// The profiles the seat negotiates, most preferred first. The AEAD profile is what live
    /// sessions use; the AES-CM one is offered because some seats still select it.
    public static let srtpProfiles = "SRTP_AEAD_AES_256_GCM:SRTP_AES128_CM_SHA1_80"

    public let role: Role
    public private(set) var isConnected = false

    private let context: OpaquePointer
    private let ssl: OpaquePointer
    private let readBIO: OpaquePointer
    private let writeBIO: OpaquePointer

    public init(role: Role, identity: NvstDtlsIdentity, expectedPeerFingerprint: String?, srtpProfiles: String = NvstDtlsHandshake.srtpProfiles) throws {
        self.role = role
        let method = role == .client ? DTLS_client_method() : DTLS_server_method()
        guard let context = SSL_CTX_new(method) else { throw HandshakeError.contextUnavailable }
        self.context = context
        // `DTLS_client_method` negotiates the highest version the build supports; the seat speaks
        // DTLS 1.2, so both ends are pinned there. The setters are macros over the ctrl interface.
        SSL_CTX_ctrl(context, SSL_CTRL_SET_MIN_PROTO_VERSION, Int(DTLS1_2_VERSION), nil)
        SSL_CTX_ctrl(context, SSL_CTRL_SET_MAX_PROTO_VERSION, Int(DTLS1_2_VERSION), nil)
        // `SSL_CTX_set_read_ahead` is a macro over the ctrl interface, which Swift can call directly.
        SSL_CTX_ctrl(context, SSL_CTRL_SET_READ_AHEAD, 1, nil)
        // A self-signed leaf cannot satisfy chain validation, and the protocol does not ask it to:
        // the peer is authenticated by comparing its certificate's digest with the announced value.
        // `SSL_VERIFY_PEER` is still set so the certificate is actually requested and presented.
        SSL_CTX_set_verify(context, SSL_VERIFY_PEER, opennowAcceptAnyCertificate)
        guard SSL_CTX_set_tlsext_use_srtp(context, srtpProfiles) == 0 else {
            SSL_CTX_free(context)
            throw HandshakeError.srtpProfilesRejected
        }
        try identity.apply(to: context)

        guard let ssl = SSL_new(context) else {
            SSL_CTX_free(context)
            throw HandshakeError.sslUnavailable
        }
        self.ssl = ssl
        guard let readBIO = BIO_new(BIO_s_mem()), let writeBIO = BIO_new(BIO_s_mem()) else {
            SSL_free(ssl)
            SSL_CTX_free(context)
            throw HandshakeError.sslUnavailable
        }
        // A memory BIO must report "no data yet" rather than EOF, or the handshake sees the peer
        // vanish every time a flight has not arrived. `BIO_set_mem_eof_return` is a macro here.
        BIO_ctrl(readBIO, BIO_C_SET_BUF_MEM_EOF_RETURN, -1, nil)
        BIO_ctrl(writeBIO, BIO_C_SET_BUF_MEM_EOF_RETURN, -1, nil)
        // Takes ownership of both BIOs.
        SSL_set_bio(ssl, readBIO, writeBIO)
        self.readBIO = readBIO
        self.writeBIO = writeBIO
        if role == .client {
            SSL_set_connect_state(ssl)
        } else {
            SSL_set_accept_state(ssl)
        }
        self.expectedPeerFingerprint = expectedPeerFingerprint
    }

    deinit {
        SSL_free(ssl)
        SSL_CTX_free(context)
    }

    /// Seconds until the DTLS layer wants the last flight retransmitted, or nil when it is waiting
    /// on the peer or already finished. A lost datagram only recovers if the caller honours this.
    /// `DTLSv1_get_timeout` is a macro over `SSL_ctrl`, which Swift cannot call.
    public var retransmissionDelay: TimeInterval? {
        guard !isConnected else { return nil }
        var timeout = timeval()
        guard SSL_ctrl(ssl, DTLS_CTRL_GET_TIMEOUT, 0, &timeout) == 1 else { return nil }
        return TimeInterval(timeout.tv_sec) + TimeInterval(timeout.tv_usec) / 1_000_000
    }

    /// Retransmits the pending flight when the DTLS timer has expired. Returns the datagram to send,
    /// or nil when nothing was due. `DTLSv1_handle_timeout` is a macro over `SSL_ctrl` too.
    @discardableResult
    public func retransmitWhenDue() -> Data? {
        guard !isConnected, SSL_ctrl(ssl, DTLS_CTRL_HANDLE_TIMEOUT, 0, nil) == 1 else { return nil }
        let pending = drain(writeBIO)
        return pending.isEmpty ? nil : pending
    }

    /// The SRTP profile the handshake settled on, as OpenSSL names it (`SRTP_AEAD_AES_256_GCM`), or
    /// nil before the handshake or when the peer offered none.
    public var selectedSrtpProfileName: String? {
        guard isConnected, let profile = SSL_get_selected_srtp_profile(ssl), let name = profile.pointee.name else { return nil }
        return String(cString: name)
    }

    /// Feeds one inbound datagram to the record layer once the handshake is done. Handshake records
    /// that arrive late are processed here too, which is what `SSL_read` returns nothing for.
    public func feedDatagram(_ datagram: Data) throws {
        guard !datagram.isEmpty else { return }
        let written = datagram.withUnsafeBytes { BIO_write(readBIO, $0.baseAddress, Int32($0.count)) }
        guard written == datagram.count else {
            throw HandshakeError.handshakeFailed("could not feed an inbound datagram to DTLS")
        }
    }

    /// Encrypts application data and returns the datagram to send, or nil when there is nothing.
    public func writeApplicationData(_ payload: Data) throws -> Data? {
        guard isConnected else { throw HandshakeError.notConnected }
        guard !payload.isEmpty else { return nil }
        let written = payload.withUnsafeBytes { SSL_write(ssl, $0.baseAddress, Int32($0.count)) }
        guard written > 0 else { throw HandshakeError.handshakeFailed(Self.lastErrorDescription()) }
        let pending = drain(writeBIO)
        return pending.isEmpty ? nil : pending
    }

    /// Decrypts one application datagram, or nil when the record layer has no complete one yet.
    public func readApplicationData() throws -> Data? {
        guard isConnected else { throw HandshakeError.notConnected }
        var buffer = [UInt8](repeating: 0, count: 4096)
        let read = buffer.withUnsafeMutableBytes { SSL_read(ssl, $0.baseAddress, Int32($0.count)) }
        if read > 0 { return Data(buffer[0..<Int(read)]) }
        let error = SSL_get_error(ssl, read)
        guard error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE else {
            throw HandshakeError.handshakeFailed(Self.lastErrorDescription())
        }
        return nil
    }

    private let expectedPeerFingerprint: String?

    /// Whether the read BIO still holds bytes the record layer has not consumed.
    ///
    /// A single DTLS datagram can carry a whole flight, and OpenSSL processes one record per call,
    /// so a driver that calls in once per datagram stalls one record short of finishing — with no
    /// error to explain it. This is what tells such a driver to call in again.
    public var hasPendingInbound: Bool {
        BIO_ctrl(readBIO, BIO_CTRL_PENDING, 0, nil) > 0
    }

    /// Advances the handshake by one flight. `received` is a datagram from the peer, or nil to
    /// re-drive after a timeout. Returns the datagram to send, or nil when there is nothing to send.
    @discardableResult
    public func handshakeStep(received: Data?) throws -> Data? {
        if let received, !received.isEmpty {
            let written = received.withUnsafeBytes { bytes in
                BIO_write(readBIO, bytes.baseAddress, Int32(bytes.count))
            }
            guard written == received.count else {
                throw HandshakeError.handshakeFailed("could not feed an inbound datagram to DTLS")
            }
        }
        let result = SSL_do_handshake(ssl)
        if result == 1 {
            try completeHandshake()
            let pending = drain(writeBIO)
            return pending.isEmpty ? nil : pending
        }
        let error = SSL_get_error(ssl, result)
        guard error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE else {
            throw HandshakeError.handshakeFailed(Self.lastErrorDescription())
        }
        let pending = drain(writeBIO)
        return pending.isEmpty ? nil : pending
    }

    /// The negotiated keying material, labelled for SRTP exactly as RFC 5764 requires.
    public func exportKeyingMaterial(length: Int) throws -> Data {
        guard isConnected else { throw HandshakeError.notConnected }
        guard length > 0 else { throw HandshakeError.keyingExportFailed }
        var out = [UInt8](repeating: 0, count: length)
        let label = Self.keyingMaterialLabel
        let exported = out.withUnsafeMutableBufferPointer { buffer in
            SSL_export_keying_material(ssl, buffer.baseAddress, buffer.count, label, strlen(label), nil, 0, 0)
        }
        guard exported == 1 else { throw HandshakeError.keyingExportFailed }
        return Data(out)
    }

    /// The peer's certificate digest in the announced colon-hex form, once the handshake is done.
    public private(set) var peerFingerprint: String?

    static let keyingMaterialLabel = "EXTRACTOR-dtls_srtp"

    private func completeHandshake() throws {
        let fingerprint = try Self.peerCertificateFingerprint(ssl)
        peerFingerprint = fingerprint
        if let expected = expectedPeerFingerprint, !expected.isEmpty,
           fingerprint.caseInsensitiveCompare(expected) != .orderedSame {
            throw HandshakeError.fingerprintMismatch(expected: expected, actual: fingerprint)
        }
        isConnected = true
    }

    /// `SSL_get_peer_certificate` is a macro in this OpenSSL; the real accessor is `SSL_get1_peer_certificate`.
    private static func peerCertificateFingerprint(_ ssl: OpaquePointer) throws -> String {
        guard let certificate = SSL_get1_peer_certificate(ssl) else {
            throw HandshakeError.peerPresentedNoCertificate
        }
        defer { X509_free(certificate) }
        var digest = [UInt8](repeating: 0, count: Int(EVP_MAX_MD_SIZE))
        var length: UInt32 = 0
        let status = digest.withUnsafeMutableBufferPointer { buffer in
            X509_digest(certificate, EVP_sha256(), buffer.baseAddress, &length)
        }
        guard status == 1, length > 0 else {
            throw HandshakeError.peerPresentedNoCertificate
        }
        return digest.prefix(Int(length)).map { String(format: "%02X", $0) }.joined(separator: ":")
    }

    private func drain(_ bio: OpaquePointer) -> Data {
        var collected = Data()
        var buffer = [UInt8](repeating: 0, count: 2048)
        while BIO_ctrl(bio, BIO_CTRL_PENDING, 0, nil) > 0 {
            let read = buffer.withUnsafeMutableBytes { bytes in
                BIO_read(bio, bytes.baseAddress, Int32(bytes.count))
            }
            guard read > 0 else { break }
            collected.append(contentsOf: buffer[0..<Int(read)])
        }
        return collected
    }

    private static func lastErrorDescription() -> String {
        let code = ERR_get_error()
        guard code != 0 else { return "no OpenSSL error recorded" }
        var text = [CChar](repeating: 0, count: 256)
        ERR_error_string_n(code, &text, text.count)
        return String(decoding: text.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self)
    }
}
