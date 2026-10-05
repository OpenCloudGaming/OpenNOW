import Foundation

/// The SRTP master values a completed DTLS-SRTP handshake yields.
///
/// RFC 5764 §4.2 fixes the layout of `SSL_export_keying_material`'s output: the two write keys
/// followed by the two write salts, each doubled for the two directions. Getting the order wrong
/// produces a session where media is decrypted with the peer's key — nothing errors, the audio is
/// just noise — so the split is stated once, here, and tested.
public struct NvstBundleSrtpKeys: Equatable, Sendable {
    public enum SplitError: LocalizedError, Equatable {
        case wrongLength(expected: Int, actual: Int)

        public var errorDescription: String? {
            switch self {
            case .wrongLength(let expected, let actual):
                "DTLS exported \(actual) bytes of keying material; \(expected) were expected for this SRTP profile."
            }
        }
    }

    public let clientMasterKey: Data
    public let clientMasterSalt: Data
    public let serverMasterKey: Data
    public let serverMasterSalt: Data

    /// How many bytes the handshake must export for `profile`.
    public static func exportedLength(for profile: NVSTSrtpProfile) -> Int {
        2 * (profile.masterKeyLength + profile.masterSaltLength)
    }

    public static func split(_ exported: Data, profile: NVSTSrtpProfile) throws -> NvstBundleSrtpKeys {
        let keyLength = profile.masterKeyLength
        let saltLength = profile.masterSaltLength
        let expected = exportedLength(for: profile)
        guard exported.count == expected, keyLength > 0, saltLength > 0 else {
            throw SplitError.wrongLength(expected: expected, actual: exported.count)
        }
        let clientKeyStart = 0
        let serverKeyStart = clientKeyStart + keyLength
        let clientSaltStart = serverKeyStart + keyLength
        let serverSaltStart = clientSaltStart + saltLength
        return NvstBundleSrtpKeys(
            clientMasterKey: exported.subdata(in: clientKeyStart..<serverKeyStart),
            clientMasterSalt: exported.subdata(in: clientSaltStart..<serverSaltStart),
            serverMasterKey: exported.subdata(in: serverKeyStart..<clientSaltStart),
            serverMasterSalt: exported.subdata(in: serverSaltStart..<serverSaltStart + saltLength)
        )
    }
}
