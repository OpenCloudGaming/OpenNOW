// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import com.opencloudgaming.opennow.AuthUser
import com.opencloudgaming.opennow.ColorQuality
import com.opencloudgaming.opennow.OpenNowJson
import com.opencloudgaming.opennow.StreamSettings

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import kotlinx.serialization.Serializable
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val AUTH_CIPHER_PREFIX = "opennow-boosteroid-v1:"
private const val AUTH_KEY_ALIAS = "opennow-boosteroid-auth"

@Serializable
internal data class BoosteroidTokens(
    val accessToken: String,
    val refreshToken: String?,
    val authDataToken: String? = null,
)

@Serializable
internal data class BoosteroidAccount(
    val tokens: BoosteroidTokens,
    val user: AuthUser,
)

internal class BoosteroidAccountStore(context: Context) {
    private val prefs = context.getSharedPreferences("opennow_boosteroid", Context.MODE_PRIVATE)
    @Volatile var account: BoosteroidAccount? = prefs.getString("account", null)?.let { encoded ->
        runCatching { OpenNowJson.decodeFromString<BoosteroidAccount>(BoosteroidAuthCipher.decrypt(encoded)) }.getOrNull()
    }
        private set

    @Synchronized
    fun save(next: BoosteroidAccount?) {
        val editor = prefs.edit()
        if (next == null) editor.remove("account")
        else editor.putString("account", BoosteroidAuthCipher.encrypt(OpenNowJson.encodeToString(next)))
        editor.apply()
        account = next
    }

    @Synchronized
    fun replaceIfCurrent(expected: BoosteroidAccount, next: BoosteroidAccount?): Boolean {
        if (account != expected) return false
        save(next)
        return true
    }

    fun loadSettings(): StreamSettings = prefs.getString("stream_settings", null)?.let {
        runCatching { OpenNowJson.decodeFromString<StreamSettings>(it) }.getOrNull()
    } ?: StreamSettings(colorQuality = ColorQuality.EightBit420)

    fun saveSettings(settings: StreamSettings) {
        prefs.edit().putString("stream_settings", OpenNowJson.encodeToString(settings)).apply()
    }
}

private object BoosteroidAuthCipher {
    private const val ANDROID_KEY_STORE = "AndroidKeyStore"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val TAG_BITS = 128

    fun encrypt(plaintext: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val ciphertext = cipher.doFinal(plaintext.toByteArray(Charsets.UTF_8))
        require(cipher.iv.size in 1..255) { "Unsupported authentication IV length." }
        val payload = ByteArray(1 + cipher.iv.size + ciphertext.size)
        payload[0] = cipher.iv.size.toByte()
        cipher.iv.copyInto(payload, destinationOffset = 1)
        ciphertext.copyInto(payload, destinationOffset = 1 + cipher.iv.size)
        return AUTH_CIPHER_PREFIX + Base64.encodeToString(payload, Base64.NO_WRAP)
    }

    fun decrypt(value: String): String {
        require(value.startsWith(AUTH_CIPHER_PREFIX)) { "Unrecognized Boosteroid credential format." }
        val payload = Base64.decode(value.removePrefix(AUTH_CIPHER_PREFIX), Base64.NO_WRAP)
        val ivSize = payload.firstOrNull()?.toInt()?.and(0xff) ?: 0
        require(ivSize > 0 && payload.size > 1 + ivSize) { "Encrypted auth state is incomplete." }
        val iv = payload.copyOfRange(1, 1 + ivSize)
        val ciphertext = payload.copyOfRange(1 + ivSize, payload.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_BITS, iv))
        return cipher.doFinal(ciphertext).toString(Charsets.UTF_8)
    }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEY_STORE).apply { load(null) }
        (keyStore.getKey(AUTH_KEY_ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEY_STORE).run {
            init(
                KeyGenParameterSpec.Builder(
                    AUTH_KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true)
                    .build(),
            )
            generateKey()
        }
    }
}
