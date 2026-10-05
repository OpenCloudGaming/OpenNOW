import Darwin
import Foundation

/// An ephemeral UDP socket held open so ANNOUNCE can advertise its port without racing a rebind.
///
/// The descriptor is transferred (not re-bound) to whoever ends up reading the socket: the seat
/// keys its NAT mapping to the exact source port that punched, so closing and reopening the port
/// between ANNOUNCE and the first packet loses the mapping.
public final class NvstUdpPortReservation: @unchecked Sendable {
    public enum ReservationError: LocalizedError, Equatable, Sendable {
        case bind(String)

        public var errorDescription: String? {
            switch self {
            case .bind(let reason): "NVST UDP reservation failed: \(reason)"
            }
        }
    }

    let lock = NSLock()
    private var descriptor: Int32
    public let port: UInt16

    private init(descriptor: Int32, port: UInt16) {
        self.descriptor = descriptor
        self.port = port
    }

    public static func bindEphemeral() throws -> NvstUdpPortReservation {
        let socketDescriptor = socket(AF_INET, SOCK_DGRAM, 0)
        guard socketDescriptor >= 0 else { throw ReservationError.bind(String(cString: strerror(errno))) }
        var reuse: Int32 = 1
        setsockopt(socketDescriptor, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr.s_addr = INADDR_ANY
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                bind(socketDescriptor, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0 else {
            let reason = String(cString: strerror(errno))
            close(socketDescriptor)
            throw ReservationError.bind(reason)
        }
        var local = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let named = withUnsafeMutablePointer(to: &local) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                getsockname(socketDescriptor, sockaddrPointer, &length)
            }
        }
        guard named == 0 else {
            let reason = String(cString: strerror(errno))
            close(socketDescriptor)
            throw ReservationError.bind(reason)
        }
        return NvstUdpPortReservation(descriptor: socketDescriptor, port: UInt16(bigEndian: local.sin_port))
    }

    /// Transfers socket ownership to the caller. Returns -1 once taken or released.
    public func takeDescriptor() -> Int32 {
        lock.lock()
        defer { lock.unlock() }
        let taken = descriptor
        descriptor = -1
        return taken
    }

    public func release() {
        lock.lock()
        let closing = descriptor
        descriptor = -1
        lock.unlock()
        if closing >= 0 { close(closing) }
    }
}

/// Reserves the two sockets of the official cloud model locally: the ICE/DTLS bundle socket and
/// the dedicated raw-SRTP Mjolnir video socket.
///
/// The reserved bundle socket is a placeholder: when the real ICE/DTLS bundle is supplied it binds
/// its own socket, and this reservation only carries the shared local ICE credentials (both sockets
/// authenticate with one 4-character ufrag) plus the Mjolnir descriptor.
public final class NvstLocalBundleReserver: NvstBundleReserving, @unchecked Sendable {
    let lock = NSLock()
    var bundle: NvstUdpPortReservation?
    private var mjolnir: NvstUdpPortReservation?
    private let dtlsFingerprint: String?
    private let iceCredentials: NvstRtspIceCredentials

    /// Brings up the ICE/DTLS bundle once the negotiated handoff is known. Set by the transport;
    /// nil keeps the pre-bundle behaviour.
    private let bundleProvider: (@Sendable (NVSTVideoHandoff, Bool) async -> NvstBundleReservation?)?

    public init(iceCredentials: NvstRtspIceCredentials = NvstRtspSdp.generateIceCredentials(),
                dtlsFingerprint: String? = nil,
                bundleProvider: (@Sendable (NVSTVideoHandoff, Bool) async -> NvstBundleReservation?)? = nil) {
        self.iceCredentials = iceCredentials
        self.dtlsFingerprint = dtlsFingerprint
        self.bundleProvider = bundleProvider
    }

    /// The shared local ICE credentials. The Mjolnir socket punches with the same ufrag the bundle
    /// authenticates with, which is what the official capture shows.
    public var localIceCredentials: NvstRtspIceCredentials { iceCredentials }

    public func bundleIdentity(for handoff: NVSTVideoHandoff, microphoneOfferedOnBundle: Bool) async -> NvstBundleReservation? {
        await bundleProvider?(handoff, microphoneOfferedOnBundle)
    }

    public func reserveBundle() async throws -> NvstBundleReservation {
        let bundleSocket = try NvstUdpPortReservation.bindEphemeral()
        let mjolnirSocket: NvstUdpPortReservation
        do {
            mjolnirSocket = try NvstUdpPortReservation.bindEphemeral()
        } catch {
            bundleSocket.release()
            throw error
        }
        // Audio stays on the bundle, where libwebrtc handles it. Reserving a socket of our own was
        // tried at length and removed: the seat accepts `SETUP streamid=audio/0` and reads
        // `general.clientPorts.audio`, then routes no audio to it - and publishing that port flips
        // `carriesAudioOnBundle` false, which also stops the seat sending *video* (repeated
        // "receiver-report tick produced nothing (ssrc=unbound)", no picture). `audioPort` stays in
        // the reservation type as nil so ANNOUNCE keeps the shape a working session negotiates.
        store(bundle: bundleSocket, mjolnir: mjolnirSocket)
        return NvstBundleReservation(
            bundlePort: bundleSocket.port,
            mjolnirPort: mjolnirSocket.port,
            audioPort: nil,
            localAddress: NvstRoutedIPv4.discover(),
            iceCredentials: iceCredentials,
            dtlsFingerprint: dtlsFingerprint
        )
    }

    private func store(bundle: NvstUdpPortReservation,
                       mjolnir: NvstUdpPortReservation) {
        lock.withLock {
            self.bundle = bundle
            self.mjolnir = mjolnir
        }
    }

    /// Hands the Mjolnir socket to the video receiver, preserving the NAT mapping.
    public func takeMjolnirDescriptor() -> Int32 {
        lock.lock()
        defer { lock.unlock() }
        return mjolnir?.takeDescriptor() ?? -1
    }

    public func takeBundleDescriptor() -> Int32 {
        lock.lock()
        defer { lock.unlock() }
        return bundle?.takeDescriptor() ?? -1
    }

    public func release() {
        lock.lock()
        let sockets = [bundle, mjolnir].compactMap { $0 }
        bundle = nil
        mjolnir = nil
        lock.unlock()
        sockets.forEach { $0.release() }
    }
}
