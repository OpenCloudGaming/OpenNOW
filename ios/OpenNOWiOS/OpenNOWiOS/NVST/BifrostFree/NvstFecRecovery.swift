import Foundation

/// Repairs lost video packets locally from the seat's Reed-Solomon repair stream, so a
/// single-packet loss never becomes a broken reference chain, a keyframe request, or a knock to
/// the seat's frame pacing. About 10% of the stream's bandwidth is this repair data; every byte
/// of it was previously parsed and thrown away.
///
/// Wire model (the one Moonlight has decoded NVIDIA FEC with for years): a shard is one whole
/// plaintext packet zero-padded to the block's uniform size, the code is nanors-compatible RS
/// over GF(2^8)/0x11D, and the sender overlays real transport headers onto its parity packets —
/// so parity is only trustworthy from the payload region onward, and a recovered packet's header
/// is rebuilt deterministically from the block instead (every header field of a missing source
/// packet is implied by its position in the block).
///
/// **Self-verifying:** complete blocks are re-encoded and compared with the parity the seat
/// actually sent over the payload region. Recovery arms only after `verificationTarget` blocks
/// match byte for byte and disarms permanently on any mismatch. Until armed — or if the scheme
/// ever disagrees — behavior is identical to not having FEC at all.
public final class NvstFecRecovery: @unchecked Sendable {
    public struct Findings: Sendable, Equatable {
        /// Complete blocks whose re-encoded parity matched the seat's parity byte for byte.
        public var verifiedBlocks = 0
        /// Complete blocks whose parity disagreed. One of these disables recovery for the session.
        public var mismatchedBlocks = 0
        /// Source packets reconstructed and handed back into the receive path.
        public var recoveredPackets = 0
        /// Blocks that lost more shards than the parity could repair.
        public var unrecoverableBlocks = 0
        /// Recovery attempts that failed structurally (size disagreement, singular system).
        public var failedRecoveries = 0
        public var isArmed = false

        public var summary: String {
            "fec verified=\(verifiedBlocks) mismatch=\(mismatchedBlocks) recovered=\(recoveredPackets)"
                + " unrecoverable=\(unrecoverableBlocks) failed=\(failedRecoveries) armed=\(isArmed)"
        }
    }

    /// Clean complete blocks required before recovery is trusted.
    public static let verificationTarget = 4
    /// Verification keeps sampling this many blocks after arming, as ongoing assurance.
    static let verificationCeiling = verificationTarget * 4
    /// RTP header (12) + extension header (4) + GS block (16): where the payload region starts.
    static let headerLength = 32
    /// Blocks kept in flight. Packets of a block arrive back to back; two blocks of slack cover
    /// reordering across a block boundary.
    private static let retainedBlocks = 3

    private struct Block {
        /// Stored as `Data` so the per-packet observe shares the decrypt path's buffer instead of
        /// copying ~1.4 KB into a fresh `[UInt8]`; the byte arrays are only materialized when a
        /// block is actually verified or recovered.
        var shards: [UInt32: Data] = [:]
        var sourceCount: Int
        var parityCount: Int
        var percentage: UInt32
        var currentBlock: UInt8
        var lastBlock: UInt8
        /// Header template from the first source packet seen, plus the fields that vary by
        /// position, so a missing packet's header can be rebuilt exactly.
        var templateHeader: [UInt8]?
        var templateFecIndex: UInt32 = 0
        var templateSequence: UInt16 = 0
        var templateStreamSequence: UInt32 = 0
        var recoveryDone = false
    }

    let lock = NSLock()
    private var findings = Findings()
    private var blocks: [UInt64: Block] = [:]
    private var blockOrder: [UInt64] = []
    private var disabled = false

    public init() {}

    public var snapshot: Findings {
        lock.lock()
        defer { lock.unlock() }
        return findings
    }

    /// Feeds one authenticated, parsed packet (its full decrypted plaintext, RTP header included).
    /// Returns reconstructed plaintext packets when this packet completed a repairable block.
    public func observe(plaintext: Data, packet: NvstRtpVideoPacket) -> [Data] {
        guard packet.fecPercentage > 0, packet.fecSourcePackets > 0,
              plaintext.count > Self.headerLength else { return [] }
        lock.lock()
        defer { lock.unlock() }
        guard !disabled else { return [] }

        let sourceCount = Int(packet.fecSourcePackets)
        let parityCount = (sourceCount * Int(packet.fecPercentage) + 99) / 100
        guard parityCount > 0, sourceCount + parityCount <= NvstReedSolomon.maximumShards,
              packet.fecIndex < UInt32(sourceCount + parityCount) else { return [] }

        let key = UInt64(packet.frameIndex) << 8 | UInt64(packet.fecCurrentBlock)
        if blocks[key] == nil {
            startBlockLocked(key: key, packet: packet, sourceCount: sourceCount, parityCount: parityCount)
        }
        // Removed from the dictionary for the duration of the mutation: while it is out, the
        // block's shard storage is uniquely owned, so the insert below updates in place instead
        // of copying the whole shards dictionary (a CoW of every shard seen so far, per packet).
        guard var block = blocks.removeValue(forKey: key) else { return [] }
        defer { blocks[key] = block }
        guard block.sourceCount == sourceCount, block.parityCount == parityCount else { return [] }
        ingest(plaintext: plaintext, packet: packet, into: &block, sourceCount: sourceCount)
        verifyIfComplete(&block)
        return recover(&block, sourceCount: sourceCount, parityCount: parityCount)
    }

    /// Registers a block never seen before, evicting the oldest once the retention window is full.
    /// The caller already holds `lock`.
    private func startBlockLocked(key: UInt64, packet: NvstRtpVideoPacket, sourceCount: Int, parityCount: Int) {
        blocks[key] = Block(sourceCount: sourceCount,
                            parityCount: parityCount,
                            percentage: packet.fecPercentage,
                            currentBlock: packet.fecCurrentBlock,
                            lastBlock: packet.fecLastBlock)
        blockOrder.append(key)
        while blockOrder.count > Self.retainedBlocks {
            let evicted = blocks.removeValue(forKey: blockOrder.removeFirst())
            if let evicted, !evicted.recoveryDone,
               evicted.shards.count < evicted.sourceCount {
                findings.unrecoverableBlocks += 1
            }
        }
    }

    /// Stores the shard, and adopts the first source packet seen as the header template used to
    /// rebuild the headers of the missing ones.
    private func ingest(plaintext: Data, packet: NvstRtpVideoPacket, into block: inout Block, sourceCount: Int) {
        block.shards[packet.fecIndex] = plaintext
        guard block.templateHeader == nil, packet.fecIndex < UInt32(sourceCount) else { return }
        block.templateHeader = [UInt8](plaintext.prefix(Self.headerLength))
        block.templateFecIndex = packet.fecIndex
        block.templateSequence = packet.sequenceNumber
        block.templateStreamSequence = packet.streamSequence
    }

    /// Repairs as soon as enough shards exist: with one loss that is the moment the first parity
    /// packet lands — long before the reorder window would finalize the gap.
    private func recover(_ block: inout Block, sourceCount: Int, parityCount: Int) -> [Data] {
        guard findings.isArmed, !block.recoveryDone, block.templateHeader != nil else { return [] }
        let missingSources = (0..<sourceCount).filter { block.shards[UInt32($0)] == nil }
        guard !missingSources.isEmpty, block.shards.count >= sourceCount else { return [] }

        let size = block.shards.values.map(\.count).max() ?? 0
        guard size > Self.headerLength else { return [] }
        var slots = [[UInt8]?](repeating: nil, count: sourceCount + parityCount)
        for (index, shard) in block.shards {
            let bytes = [UInt8](shard)
            slots[Int(index)] = bytes.count == size
                ? bytes
                : bytes + [UInt8](repeating: 0, count: size - bytes.count)
        }
        guard let recovered = NvstReedSolomon.recover(shards: &slots, dataCount: sourceCount,
                                                      parityCount: parityCount, size: size),
              !recovered.isEmpty else {
            findings.failedRecoveries += 1
            block.recoveryDone = true
            return []
        }
        block.recoveryDone = true
        findings.recoveredPackets += recovered.count
        let rebuilt = block
        return recovered.keys.sorted().compactMap { index in
            recovered[index].map { shard in
                Data(rebuiltHeader(for: rebuilt, missingIndex: UInt32(index)))
                    + Data(shard[Self.headerLength...])
            }
        }
    }

    /// Rebuilds the 32-byte header the sender overlaid on its parity packets: every field of a
    /// missing source packet's header is implied by its position in the block.
    private func rebuiltHeader(for block: Block, missingIndex: UInt32) -> [UInt8] {
        var header = block.templateHeader ?? [UInt8](repeating: 0, count: Self.headerLength)
        // RTP sequence number (big-endian at +2): consecutive across the block's sources.
        let sequence = block.templateSequence &+ UInt16(truncatingIfNeeded: missingIndex)
            &- UInt16(truncatingIfNeeded: block.templateFecIndex)
        header[2] = UInt8(sequence >> 8)
        header[3] = UInt8(sequence & 0xff)
        // GS stream packet index (little-endian 24-bit at +17, above the low flag byte at +16):
        // consecutive across the frame's source packets.
        let streamSequence = (block.templateStreamSequence &+ missingIndex &- block.templateFecIndex) & 0xff_ffff
        header[17] = UInt8(streamSequence & 0xff)
        header[18] = UInt8((streamSequence >> 8) & 0xff)
        header[19] = UInt8((streamSequence >> 16) & 0xff)
        // Flags nibble (low nibble of +24): picture data always; start-of-frame only for the
        // frame's very first packet; end-of-frame only for the last source of the last block.
        // Bytes +25/+26 keep the template's values; +27 carries the multi-FEC block counters.
        var flags: UInt8 = 0x01
        if missingIndex == 0, block.currentBlock == 0 { flags |= 0x04 }
        if Int(missingIndex) == block.sourceCount - 1, block.currentBlock == block.lastBlock { flags |= 0x02 }
        header[24] = (header[24] & 0xf0) | flags
        header[27] = (header[27] & 0x0f) | (block.currentBlock << 4) | (block.lastBlock << 6)
        // FEC word (little-endian at +28): this packet's own index, the block's source count and
        // percentage; the low nibble is preserved from the template.
        let fecWord = (block.templateHeader.map { UInt32($0[28]) & 0x0f } ?? 0)
            | (block.percentage << 4)
            | (missingIndex << 12)
            | (UInt32(block.sourceCount) << 22)
        header[28] = UInt8(fecWord & 0xff)
        header[29] = UInt8((fecWord >> 8) & 0xff)
        header[30] = UInt8((fecWord >> 16) & 0xff)
        header[31] = UInt8((fecWord >> 24) & 0xff)
        return header
    }

    /// Re-encodes a complete block and compares against the seat's own parity over the payload
    /// region (the header region of a parity packet carries its real transport headers, not
    /// parity). This is what makes enabling recovery safe: the convention is proven on this very
    /// stream before it is used.
    private func verifyIfComplete(_ block: inout Block) {
        guard findings.verifiedBlocks < Self.verificationCeiling else { return }
        guard !block.recoveryDone, block.shards.count == block.sourceCount + block.parityCount else { return }
        block.recoveryDone = true

        let size = block.shards.values.map(\.count).max() ?? 0
        guard size > Self.headerLength else { return }
        func padded(_ shard: Data) -> [UInt8] {
            let bytes = [UInt8](shard)
            return bytes.count == size ? bytes : bytes + [UInt8](repeating: 0, count: size - bytes.count)
        }
        let data = (0..<block.sourceCount).compactMap { block.shards[UInt32($0)].map(padded) }
        let parity = (0..<block.parityCount).compactMap { block.shards[UInt32(block.sourceCount + $0)].map(padded) }
        guard data.count == block.sourceCount, parity.count == block.parityCount,
              let expected = NvstReedSolomon.encode(data: data, parityCount: block.parityCount, size: size) else { return }
        let payloadMatches = zip(expected, parity).allSatisfy { expectedShard, receivedShard in
            expectedShard[Self.headerLength...] == receivedShard[Self.headerLength...]
        }
        if payloadMatches {
            findings.verifiedBlocks += 1
            if findings.verifiedBlocks >= Self.verificationTarget { findings.isArmed = true }
        } else {
            findings.mismatchedBlocks += 1
            findings.isArmed = false
            disabled = true
        }
    }
}
