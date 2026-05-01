package io.burniq.medium.android

import android.content.Intent
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.util.Log

class MediumRoutingService : VpnService() {
    private var tunnel: ParcelFileDescriptor? = null
    private var nativeHandle: Long = 0

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> stopTunnel()
            else -> startTunnel(intent?.getStringExtra(EXTRA_SERVICES_JSON).orEmpty())
        }
        return START_STICKY
    }

    override fun onDestroy() {
        stopTunnel(stopService = false)
        super.onDestroy()
    }

    private fun startTunnel(servicesJson: String) {
        if (tunnel != null) {
            stopTunnel(stopService = false)
        }
        if (!MediumNativeBridge.isAvailable()) {
            Log.e(TAG, "native netstack library is not available")
            stopSelf()
            return
        }
        val established = Builder()
            .setSession("Medium")
            .addAddress("10.88.0.2", 24)
            .addDnsServer("10.88.0.1")
            .addRoute("10.88.0.0", 24)
            .establish()
        if (established == null) {
            Log.e(TAG, "failed to establish Medium routing tunnel")
            stopSelf()
            return
        }
        val fd = established.detachFd()
        val handle = MediumNativeBridge.startTun(this, fd, servicesJson)
        if (handle == 0L) {
            Log.e(TAG, "native netstack failed to start")
            ParcelFileDescriptor.adoptFd(fd).close()
            stopSelf()
            return
        }
        tunnel = established
        nativeHandle = handle
    }

    private fun stopTunnel(stopService: Boolean = true) {
        MediumNativeBridge.stopTun(nativeHandle)
        nativeHandle = 0
        tunnel?.close()
        tunnel = null
        if (stopService) {
            stopSelf()
        }
    }

    companion object {
        private const val TAG = "MediumRoutingService"
        const val ACTION_START = "io.burniq.medium.android.START_ROUTING"
        const val ACTION_STOP = "io.burniq.medium.android.STOP_ROUTING"
        const val EXTRA_SERVICES_JSON = "io.burniq.medium.android.SERVICES_JSON"
    }
}

internal fun mediumRoutingServicesJson(
    devices: List<DeviceRecord>,
    sessionGrants: Map<String, String> = emptyMap(),
    controlPin: String? = null,
): String {
    return devices
        .flatMap { it.services }
        .joinToString(separator = ",", prefix = "[", postfix = "]") { service ->
            val label = service.label?.let { "\"${jsonEscape(it)}\"" } ?: "null"
            val grant = sessionGrants[service.id]?.let { ",\"grant\":$it" }.orEmpty()
            val pin = if (grant.isNotEmpty() && !controlPin.isNullOrBlank()) {
                ",\"control_pin\":\"${jsonEscape(controlPin)}\""
            } else {
                ""
            }
            "{\"id\":\"${jsonEscape(service.id)}\",\"label\":$label,\"kind\":\"${jsonEscape(service.kind)}\",\"target\":\"${jsonEscape(service.target)}\"$grant$pin}"
        }
}

private fun jsonEscape(value: String): String =
    buildString {
        value.forEach { ch ->
            when (ch) {
                '\\' -> append("\\\\")
                '"' -> append("\\\"")
                '\b' -> append("\\b")
                '\u000C' -> append("\\f")
                '\n' -> append("\\n")
                '\r' -> append("\\r")
                '\t' -> append("\\t")
                else -> {
                    if (ch.code < 0x20) {
                        append("\\u")
                        append(ch.code.toString(16).padStart(4, '0'))
                    } else {
                        append(ch)
                    }
                }
            }
        }
    }
