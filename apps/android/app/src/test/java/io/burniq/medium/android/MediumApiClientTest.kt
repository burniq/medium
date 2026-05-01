package io.burniq.medium.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MediumApiClientTest {
    @Test
    fun nullableStringValueTreatsJsonNullStringAsMissing() {
        assertNull(nullableStringValue(value = "null", isNull = true))
        assertNull(nullableStringValue(value = "null", isNull = false))
        assertNull(nullableStringValue(value = "", isNull = false))
        assertEquals("hello", nullableStringValue(value = "hello", isNull = false))
    }

    @Test
    fun serviceWithoutLabelUsesIdForHostname() {
        val service = PublishedService(
            id = "hello",
            kind = "https",
            schemaVersion = 1,
            label = null,
            target = "127.0.0.1:8082",
            userName = null,
        )

        assertEquals("hello", service.displayName)
        assertEquals("hello.medium", service.hostname)
    }

    @Test
    fun browserUrlUsesHttpsForWebServices() {
        val httpsService = PublishedService(
            id = "hello",
            kind = "https",
            schemaVersion = 1,
            label = null,
            target = "127.0.0.1:8082",
            userName = null,
        )
        val httpService = PublishedService(
            id = "docs",
            kind = "http",
            schemaVersion = 1,
            label = "Docs",
            target = "127.0.0.1:8080",
            userName = null,
        )

        assertEquals("https://hello.medium/", httpsService.browserUrl)
        assertEquals("https://docs.medium/", httpService.browserUrl)
    }

    @Test
    fun mediumCaPemIsDecodedToDerBytes() {
        val pem = """
            -----BEGIN CERTIFICATE-----
            AQIDBA==
            -----END CERTIFICATE-----
        """.trimIndent()

        assertEquals(listOf(1, 2, 3, 4), mediumCaPemToDer(pem).map { it.toInt() })
    }
}
