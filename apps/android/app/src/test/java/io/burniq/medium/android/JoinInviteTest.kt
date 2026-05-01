package io.burniq.medium.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import java.net.URLEncoder
import java.nio.charset.StandardCharsets

class JoinInviteTest {
    @Test
    fun parsesPinnedTlsInvite() {
        val control = URLEncoder.encode("https://127.0.0.1:7777", StandardCharsets.UTF_8.name())
        val invite = JoinInvite.parse(
            "medium://join?v=1&control=$control&security=pinned-tls&control_pin=sha256:abcd",
        )

        assertEquals(1, invite.version)
        assertEquals("https://127.0.0.1:7777", invite.controlUrl)
        assertEquals("pinned-tls", invite.security)
        assertEquals("sha256:abcd", invite.controlPin)
    }

    @Test
    fun rejectsUnsupportedScheme() {
        assertThrows(IllegalArgumentException::class.java) {
            JoinInvite.parse("https://example.com")
        }
    }

    @Test
    fun serviceHostnameNormalizesDisplayName() {
        assertEquals("hello.medium", serviceHostname("hello"))
        assertEquals("hello-world.medium", serviceHostname("Hello World"))
        assertEquals("svc-openclaw.medium", serviceHostname("svc_openclaw"))
        assertEquals("service.medium", serviceHostname("   "))
    }
}
