package io.burniq.medium.android

import org.json.JSONArray
import org.json.JSONObject
import java.net.URLEncoder
import java.nio.charset.StandardCharsets

class MediumApiClient(private val state: MediumClientState) {
    private val http = PinnedHttpClient(state.controlPin)

    fun fetchDevices(): DeviceCatalog {
        val json = http.get(state.controlUrl.trimEnd('/') + "/api/devices")
        return parseDeviceCatalog(json)
    }

    fun fetchMediumCaPem(): String =
        http.get(state.controlUrl.trimEnd('/') + "/api/medium-ca.pem")

    fun openSession(serviceId: String): String {
        val service = encode(serviceId)
        val requester = encode(state.deviceName)
        return http.get(
            state.controlUrl.trimEnd('/') +
                "/api/sessions/open?service_id=$service&requester_device_id=$requester",
        )
    }

    internal fun parseDeviceCatalog(json: String): DeviceCatalog {
        val root = JSONObject(json)
        val devices = root.optJSONArray("devices").orEmpty().mapObjects { device ->
            DeviceRecord(
                id = device.getString("id"),
                name = device.optString("name", device.getString("id")),
                services = device.optJSONArray("services").orEmpty().mapObjects { service ->
                    PublishedService(
                        id = service.getString("id"),
                        kind = service.optString("kind", "https"),
                        schemaVersion = service.optInt("schema_version", 1),
                        label = nullableStringValue(
                            value = service.optString("label", ""),
                            isNull = service.isNull("label"),
                        ),
                        target = service.optString("target", ""),
                        userName = nullableStringValue(
                            value = service.optString("user_name", ""),
                            isNull = service.isNull("user_name"),
                        ),
                    )
                },
            )
        }
        return DeviceCatalog(devices)
    }

    private fun encode(value: String): String =
        URLEncoder.encode(value, StandardCharsets.UTF_8.name())
}

private fun JSONArray?.orEmpty(): JSONArray = this ?: JSONArray()

private fun <T> JSONArray.mapObjects(transform: (JSONObject) -> T): List<T> =
    (0 until length()).map { index -> transform(getJSONObject(index)) }

internal fun nullableStringValue(value: String, isNull: Boolean): String? =
    value.takeUnless { isNull || it.isBlank() || it == "null" }
