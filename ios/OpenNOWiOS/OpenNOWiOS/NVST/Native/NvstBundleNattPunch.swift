import Foundation

/// The STUN keepalive the seat's front end uses to tell the bundle socket from the video socket.
///
/// Both client sockets talk to the seat's one public port, and the front end demultiplexes them by
/// the STUN USERNAME: `<srvUfrag><internalPort>:<localUfrag>` routes to the video service, the same
/// with the bundle port routes to the bundle service. A socket that sends DTLS without first
/// punching with the right username has no route at all, so its ClientHello is dropped and the
/// handshake dies on its deadline — which is what a live run showed.
///
/// The DESCRIBE remote ufrag already ends in the seat's internal bundle port (it came back as
/// `e503c1fe47999`), so it is used verbatim; the video socket is the one that uses the raw SETUP
/// ping payload (`…47998`).
public enum NvstBundleNattPunch {
    /// `remoteUfrag` is the DESCRIBE ICE ufrag; `localUfrag` is the shared 4-character local one.
    public static func username(remoteUfrag: String, localUfrag: String) -> String {
        "\(remoteUfrag):\(localUfrag)"
    }

    /// One authenticated STUN Binding Request carrying that identity.
    public static func request(remoteUfrag: String,
                               localUfrag: String,
                               remotePassword: Data,
                               transactionID: Data) -> Data? {
        NvstStunHolePunch.buildBindingRequest(
            transactionID: transactionID,
            username: username(remoteUfrag: remoteUfrag, localUfrag: localUfrag),
            integrityKey: remotePassword
        )
    }

    public static func transactionID() -> Data {
        var identifier = Data(count: 12)
        for index in 0..<12 { identifier[index] = UInt8.random(in: 0...255) }
        return identifier
    }
}
