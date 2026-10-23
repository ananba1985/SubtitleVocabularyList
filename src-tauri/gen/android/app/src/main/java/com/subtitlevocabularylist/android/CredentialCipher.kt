package com.subtitlevocabularylist.android

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Keys remain in Keystore; plaintext is used only in the Rust core's memory. */
internal object CredentialCipher {
    private const val ALIAS = "svl.credentials.v1"
    private val magic = "SVLC1".toByteArray(Charsets.US_ASCII)

    fun transform(data: ByteArray, protect: Boolean): ByteArray {
        require(data.isNotEmpty() && data.size <= 65536)
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val key = (store.getKey(ALIAS, null) as? SecretKey) ?: run {
            check(protect) { "Credential key unavailable" }
            KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
                init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setKeySize(256)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true).build())
            }.generateKey()
        }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        val header = if (protect) {
            cipher.init(Cipher.ENCRYPT_MODE, key)
            check(cipher.iv.size == 12)
            magic + cipher.iv
        } else {
            require(data.size >= magic.size + 12 + 16 && data.copyOfRange(0, magic.size).contentEquals(magic))
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, data.copyOfRange(magic.size, magic.size + 12)))
            data.copyOfRange(0, magic.size + 12)
        }
        cipher.updateAAD(header)
        return if (protect) header + cipher.doFinal(data) else cipher.doFinal(data, header.size, data.size - header.size)
    }
}
