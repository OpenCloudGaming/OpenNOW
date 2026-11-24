package com.opencloudgaming.opennow

/** A remember key for a published snapshot whose contents are fixed for its lifetime. */
internal class IdentityKey(private val snapshot: Any) {
    override fun equals(other: Any?): Boolean = other is IdentityKey && snapshot === other.snapshot

    override fun hashCode(): Int = System.identityHashCode(snapshot)
}
