package io.burniq.medium.android

import android.content.Context

class MediumStateStore(context: Context) {
    private val preferences = context.getSharedPreferences("medium-state", Context.MODE_PRIVATE)

    fun load(): MediumClientState? {
        val controlUrl = preferences.getString(KEY_CONTROL_URL, null) ?: return null
        val deviceName = preferences.getString(KEY_DEVICE_NAME, null) ?: return null
        val security = preferences.getString(KEY_SECURITY, null) ?: return null
        val controlPin = preferences.getString(KEY_CONTROL_PIN, null) ?: return null
        val inviteVersion = preferences.getInt(KEY_INVITE_VERSION, 0)
        if (inviteVersion == 0) return null
        return MediumClientState(
            controlUrl = controlUrl,
            deviceName = deviceName,
            inviteVersion = inviteVersion,
            security = security,
            controlPin = controlPin,
        )
    }

    fun save(state: MediumClientState) {
        preferences.edit()
            .putString(KEY_CONTROL_URL, state.controlUrl)
            .putString(KEY_DEVICE_NAME, state.deviceName)
            .putInt(KEY_INVITE_VERSION, state.inviteVersion)
            .putString(KEY_SECURITY, state.security)
            .putString(KEY_CONTROL_PIN, state.controlPin)
            .apply()
    }

    fun clear() {
        preferences.edit().clear().apply()
    }

    private companion object {
        const val KEY_CONTROL_URL = "control_url"
        const val KEY_DEVICE_NAME = "device_name"
        const val KEY_INVITE_VERSION = "invite_version"
        const val KEY_SECURITY = "security"
        const val KEY_CONTROL_PIN = "control_pin"
    }
}
