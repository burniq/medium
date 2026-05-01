package io.burniq.medium.android

import java.net.URI
import java.net.URLDecoder
import java.nio.charset.StandardCharsets

data class JoinInvite(
    val version: Int,
    val controlUrl: String,
    val security: String,
    val controlPin: String,
) {
    companion object {
        fun parse(raw: String): JoinInvite {
            val uri = URI(raw.trim())
            require(uri.scheme == "medium" && uri.host == "join") {
                "expected medium://join invite"
            }
            val query = parseQuery(uri.rawQuery.orEmpty())
            require(query["v"] == "1") { "unsupported invite version" }
            val control = query["control"].orEmpty()
            require(control.startsWith("https://")) { "missing control URL" }
            require(query["security"] == "pinned-tls") { "unsupported invite security" }
            val controlPin = query["control_pin"].orEmpty()
            require(controlPin.startsWith("sha256:")) { "missing control pin" }
            return JoinInvite(
                version = 1,
                controlUrl = control,
                security = "pinned-tls",
                controlPin = controlPin,
            )
        }

        private fun parseQuery(query: String): Map<String, String> {
            if (query.isBlank()) return emptyMap()
            return query.split("&").mapNotNull { part ->
                val pieces = part.split("=", limit = 2)
                if (pieces.size != 2) return@mapNotNull null
                decode(pieces[0]) to decode(pieces[1])
            }.toMap()
        }

        private fun decode(value: String): String =
            URLDecoder.decode(value, StandardCharsets.UTF_8.name())
    }
}

data class MediumClientState(
    val controlUrl: String,
    val deviceName: String,
    val inviteVersion: Int,
    val security: String,
    val controlPin: String,
)

data class DeviceCatalog(val devices: List<DeviceRecord>)

data class DeviceRecord(
    val id: String,
    val name: String,
    val services: List<PublishedService>,
)

data class PublishedService(
    val id: String,
    val kind: String,
    val schemaVersion: Int,
    val label: String?,
    val target: String,
    val userName: String?,
) {
    val displayName: String
        get() = label?.takeIf { it.isNotBlank() } ?: id

    val hostname: String
        get() = serviceHostname(displayName)

    val browserUrl: String
        get() {
            val scheme = when (kind.lowercase()) {
                "http", "https" -> "https"
                else -> "http"
            }
            return "$scheme://$hostname/"
        }
}

fun serviceHostname(value: String): String {
    val normalized = buildString {
        var lastDash = false
        value.lowercase().forEach { ch ->
            when {
                ch in 'a'..'z' || ch in '0'..'9' -> {
                    append(ch)
                    lastDash = false
                }
                isNotEmpty() && !lastDash -> {
                    append('-')
                    lastDash = true
                }
            }
        }
    }.trim('-')
    return "${normalized.ifBlank { "service" }}.medium"
}
