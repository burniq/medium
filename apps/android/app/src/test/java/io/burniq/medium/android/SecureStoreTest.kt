package io.burniq.medium.android

import org.junit.Assert.assertEquals
import org.junit.Test

class SecureStoreTest {
    @Test
    fun deviceLabelKeyIsStable() {
        assertEquals("device_label", SecureStore.deviceLabelKey)
    }
}
