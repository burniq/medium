package io.burniq.medium.android

import org.junit.Assert.assertEquals
import org.junit.Test

class MediumRoutingServiceTest {
    @Test
    fun servicesJsonContainsOnlyNetstackCatalogFields() {
        val grantJson =
            """{"session_id":"sess_1","service_id":"hello","node_id":"node-1","relay_hint":null,"authorization":{"token":"tok","expires_at":"2026-04-28T12:00:00Z","candidates":[]}}"""
        val json = mediumRoutingServicesJson(
            listOf(
                DeviceRecord(
                    id = "node-1",
                    name = "node-1",
                    services = listOf(
                        PublishedService(
                            id = "hello",
                            kind = "https",
                            schemaVersion = 1,
                            label = null,
                            target = "127.0.0.1:8082",
                            userName = null,
                        ),
                        PublishedService(
                            id = "svc_docs",
                            kind = "http",
                            schemaVersion = 1,
                            label = "Docs",
                            target = "127.0.0.1:8080",
                            userName = null,
                        ),
                    ),
                ),
            ),
            mapOf("hello" to grantJson),
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )

        assertEquals(
            """[{"id":"hello","label":null,"kind":"https","target":"127.0.0.1:8082","grant":$grantJson,"control_pin":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"id":"svc_docs","label":"Docs","kind":"http","target":"127.0.0.1:8080"}]""",
            json,
        )
    }
}
