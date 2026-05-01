package io.burniq.medium.android

import android.util.Log

object MediumNativeBridge {
    private const val TAG = "MediumNativeBridge"
    private var loaded = false

    init {
        loaded = runCatching {
            System.loadLibrary("medium_android_netstack")
        }.onFailure { error ->
            Log.w(TAG, "native netstack library is not available", error)
        }.isSuccess
    }

    fun isAvailable(): Boolean = loaded

    fun startTun(service: MediumRoutingService, fd: Int, servicesJson: String): Long {
        check(loaded) { "native netstack library is not available" }
        return nativeStartTun(service, fd, servicesJson)
    }

    fun stopTun(handle: Long) {
        if (loaded && handle != 0L) {
            nativeStopTun(handle)
        }
    }

    @JvmStatic
    private external fun nativeStartTun(service: MediumRoutingService, fd: Int, servicesJson: String): Long

    @JvmStatic
    private external fun nativeStopTun(handle: Long)
}
