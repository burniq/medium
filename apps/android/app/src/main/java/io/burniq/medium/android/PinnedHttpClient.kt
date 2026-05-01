package io.burniq.medium.android

import java.net.URL
import java.security.MessageDigest
import java.security.cert.X509Certificate
import javax.net.ssl.HttpsURLConnection
import javax.net.ssl.HostnameVerifier
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager

class PinnedHttpClient(expectedPin: String) {
    private val normalizedExpectedPin = expectedPin.trim().lowercase()

    fun get(url: String): String {
        val connection = (URL(url).openConnection() as HttpsURLConnection)
        connection.sslSocketFactory = sslContext().socketFactory
        connection.hostnameVerifier = HostnameVerifier { _, _ -> true }
        connection.requestMethod = "GET"
        connection.setRequestProperty("Accept", "application/json")
        connection.connectTimeout = 15_000
        connection.readTimeout = 15_000

        val statusCode = connection.responseCode
        val stream = if (statusCode in 200..299) connection.inputStream else connection.errorStream
        val body = stream.bufferedReader().use { it.readText() }
        require(statusCode in 200..299) { "control plane returned HTTP $statusCode" }
        return body
    }

    private fun sslContext(): SSLContext {
        val context = SSLContext.getInstance("TLS")
        context.init(null, arrayOf<TrustManager>(PinnedTrustManager(normalizedExpectedPin)), null)
        return context
    }

    private class PinnedTrustManager(private val expectedPin: String) : X509TrustManager {
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) = Unit

        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
            val leaf = chain?.firstOrNull() ?: error("server did not provide a certificate")
            val actualPin = certificatePin(leaf.encoded)
            require(actualPin == expectedPin.lowercase()) {
                "certificate pin mismatch"
            }
        }

        override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
    }

    companion object {
        fun certificatePin(derBytes: ByteArray): String {
            val digest = MessageDigest.getInstance("SHA-256").digest(derBytes)
            return "sha256:" + digest.joinToString(separator = "") { "%02x".format(it) }
        }
    }
}
