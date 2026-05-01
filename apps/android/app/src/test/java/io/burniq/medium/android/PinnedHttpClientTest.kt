package io.burniq.medium.android

import org.junit.Assert.assertEquals
import org.junit.Test

class PinnedHttpClientTest {
    @Test
    fun formatsSha256CertificatePin() {
        val pin = PinnedHttpClient.certificatePin("hello".toByteArray())

        assertEquals("sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824", pin)
    }
}
