package io.burniq.medium.android

import android.content.ContentValues
import android.content.Intent
import android.net.VpnService
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.provider.Settings
import android.security.KeyChain
import java.util.Base64
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

class MainActivity : ComponentActivity() {
    private lateinit var store: MediumStateStore
    private var clientState by mutableStateOf<MediumClientState?>(null)
    private var devices by mutableStateOf<List<DeviceRecord>>(emptyList())
    private var statusText by mutableStateOf("Idle")
    private var errorText by mutableStateOf<String?>(null)

    private val routingPermissionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        if (result.resultCode == RESULT_OK) {
            startRoutingService()
        } else {
            errorText = "Android routing permission was denied."
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        store = MediumStateStore(this)
        clientState = store.load()

        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize(), color = MediumPalette.Background) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(
                                Brush.verticalGradient(
                                    colors = listOf(
                                        MediumPalette.BackgroundUpper,
                                        MediumPalette.Background,
                                    ),
                                ),
                            ),
                    ) {
                        Column(
                            modifier = Modifier
                                .fillMaxSize()
                                .statusBarsPadding()
                                .verticalScroll(rememberScrollState())
                                .padding(horizontal = 20.dp, vertical = 30.dp),
                            verticalArrangement = Arrangement.spacedBy(18.dp),
                        ) {
                            if (clientState == null) {
                                MediumHero(
                                    eyebrow = null,
                                    title = "Join",
                                    subtitle = "Paste a medium://join invite. This device will store the control certificate pin locally.",
                                )
                                JoinPanel(errorText = errorText, onJoin = ::join)
                            } else {
                                MediumHero(
                                    eyebrow = null,
                                    title = "Services",
                                    subtitle = "Published endpoints available to this node.",
                                )
                                ControlPanel(
                                    state = clientState!!,
                                    devices = devices,
                                    statusText = statusText,
                                    errorText = errorText,
                                    onRefresh = ::refreshDevices,
                                    onStartRouting = ::requestRouting,
                                    onStopRouting = ::stopRouting,
                                    onInstallCa = ::installMediumCa,
                                    onOpenService = ::openService,
                                    onReset = ::reset,
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    private fun join(inviteText: String, deviceName: String) {
        runCatching {
            val invite = JoinInvite.parse(inviteText)
            MediumClientState(
                controlUrl = invite.controlUrl,
                deviceName = deviceName.ifBlank { "android" },
                inviteVersion = invite.version,
                security = invite.security,
                controlPin = invite.controlPin,
            )
        }.onSuccess { state ->
            store.save(state)
            clientState = state
            devices = emptyList()
            errorText = null
            statusText = "Joined"
        }.onFailure { error ->
            errorText = error.message ?: "Invalid invite"
        }
    }

    private fun refreshDevices() {
        val state = clientState ?: return
        statusText = "Loading services..."
        Thread {
            runCatching { MediumApiClient(state).fetchDevices().devices }
                .onSuccess { loaded ->
                    runOnUiThread {
                        devices = loaded
                        statusText = "Loaded ${loaded.size} device(s)"
                        errorText = null
                    }
                }
                .onFailure { error ->
                    runOnUiThread {
                        statusText = "Load failed"
                        errorText = error.message ?: "Failed to load services"
                    }
                }
        }.start()
    }

    private fun requestRouting() {
        val permissionIntent = VpnService.prepare(this)
        if (permissionIntent != null) {
            routingPermissionLauncher.launch(permissionIntent)
            return
        }
        startRoutingService()
    }

    private fun startRoutingService() {
        val state = clientState ?: return
        statusText = "Preparing sessions..."
        Thread {
            runCatching {
                val api = MediumApiClient(state)
                devices
                    .flatMap { it.services }
                    .associate { service -> service.id to api.openSession(service.id) }
            }.onSuccess { grants ->
                runOnUiThread {
                    startService(
                        Intent(this, MediumRoutingService::class.java)
                            .setAction(MediumRoutingService.ACTION_START)
                            .putExtra(
                                MediumRoutingService.EXTRA_SERVICES_JSON,
                                mediumRoutingServicesJson(devices, grants, state.controlPin),
                            ),
                    )
                    statusText = "Routing started"
                    errorText = null
                }
            }.onFailure { error ->
                runOnUiThread {
                    statusText = "Session failed"
                    errorText = error.message ?: "Failed to prepare service sessions"
                }
            }
        }.start()
    }

    private fun stopRouting() {
        startService(Intent(this, MediumRoutingService::class.java).setAction(MediumRoutingService.ACTION_STOP))
        statusText = "Routing stopped"
    }

    private fun installMediumCa() {
        val state = clientState ?: return
        statusText = "Loading Medium CA..."
        Thread {
            runCatching {
                val pem = MediumApiClient(state).fetchMediumCaPem()
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    saveMediumCaToDownloads(pem)
                    MediumCaInstallAction.OpenSettings
                } else {
                    MediumCaInstallAction.InstallDirect(mediumCaPemToDer(pem))
                }
            }
                .onSuccess { action ->
                    runOnUiThread {
                        when (action) {
                            is MediumCaInstallAction.InstallDirect -> {
                                val intent = KeyChain.createInstallIntent()
                                    .putExtra(KeyChain.EXTRA_CERTIFICATE, action.der)
                                    .putExtra(KeyChain.EXTRA_NAME, "Medium CA")
                                startActivity(intent)
                                statusText = "Install CA"
                            }
                            MediumCaInstallAction.OpenSettings -> {
                                startActivity(Intent(Settings.ACTION_SECURITY_SETTINGS))
                                statusText =
                                    "Saved medium-ca.crt. Install it as a CA certificate from Settings."
                            }
                        }
                        errorText = null
                    }
                }
                .onFailure { error ->
                    runOnUiThread {
                        statusText = "CA load failed"
                        errorText = error.message ?: "Failed to load Medium CA"
                    }
                }
        }.start()
    }

    private fun saveMediumCaToDownloads(pem: String): Uri {
        val values = ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME, MEDIUM_CA_FILE_NAME)
            put(MediaStore.MediaColumns.MIME_TYPE, MEDIUM_CA_MIME_TYPE)
            put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS)
            put(MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val resolver = contentResolver
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            ?: error("Failed to create $MEDIUM_CA_FILE_NAME in Downloads")
        resolver.openOutputStream(uri)?.use { stream ->
            stream.write(pem.toByteArray(Charsets.UTF_8))
        } ?: error("Failed to write $MEDIUM_CA_FILE_NAME")
        values.clear()
        values.put(MediaStore.MediaColumns.IS_PENDING, 0)
        resolver.update(uri, values, null, null)
        return uri
    }

    private fun openService(service: PublishedService) {
        startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(service.browserUrl)))
    }

    private fun reset() {
        store.clear()
        clientState = null
        devices = emptyList()
        errorText = null
        statusText = "Idle"
    }
}

@androidx.compose.runtime.Composable
private fun JoinPanel(errorText: String?, onJoin: (String, String) -> Unit) {
    var inviteText by remember { mutableStateOf("") }
    var deviceName by remember { mutableStateOf("android") }

    Column(verticalArrangement = Arrangement.spacedBy(18.dp)) {
        errorText?.let { Text(it, color = Color(0xFFFF9C8A)) }

        MediumCard {
            FieldHeader("Device", "This name will identify the phone in your Medium network.")
            Spacer(Modifier.height(14.dp))
            MediumInput(
                value = deviceName,
                onValueChange = { deviceName = it },
                placeholder = "Device name",
                singleLine = true,
            )
        }

        MediumCard {
            FieldHeader("Invite", "Expected format starts with medium://join.")
            Spacer(Modifier.height(14.dp))
            MediumInput(
                value = inviteText,
                onValueChange = { inviteText = it },
                placeholder = "medium://join invite",
                minHeight = 190.dp,
                monospace = true,
            )
        }

        MediumButton(
            title = "Join Device",
            primary = true,
            enabled = inviteText.isNotBlank(),
            onClick = { onJoin(inviteText.trim(), deviceName.trim()) },
        )
    }
}

@androidx.compose.runtime.Composable
private fun MediumHero(eyebrow: String?, title: String, subtitle: String) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (eyebrow != null) {
            Text(
                text = eyebrow.uppercase(),
                color = MediumPalette.Accent,
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 2.sp,
            )
        }
        Text(
            text = title,
            color = MediumPalette.Ink,
            fontSize = 34.sp,
            fontWeight = FontWeight.Black,
            lineHeight = 38.sp,
        )
        Text(
            text = subtitle,
            color = MediumPalette.SecondaryText,
            fontSize = 14.sp,
            lineHeight = 19.sp,
        )
    }
}

@androidx.compose.runtime.Composable
private fun ControlPanel(
    state: MediumClientState,
    devices: List<DeviceRecord>,
    statusText: String,
    errorText: String?,
    onRefresh: () -> Unit,
    onStartRouting: () -> Unit,
    onStopRouting: () -> Unit,
    onInstallCa: () -> Unit,
    onOpenService: (PublishedService) -> Unit,
    onReset: () -> Unit,
) {
    MediumCard {
        SectionTitle("Control")
        InfoRow("URL", state.controlUrl)
        InfoRow("Device", state.deviceName)
        StatusPill(statusText)
        errorText?.let { Text(it, color = Color(0xFFFF9C8A)) }
        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MediumButton(
                title = "Refresh",
                primary = true,
                modifier = Modifier.weight(1f),
                onClick = onRefresh,
            )
            MediumButton(
                title = "Reset",
                primary = false,
                modifier = Modifier.weight(1f),
                onClick = onReset,
            )
        }
    }

    MediumCard {
        SectionTitle("Tunnel")
        Text("Starts Medium network routing for published services.", color = MediumPalette.SecondaryText)
        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MediumButton(
                title = "Start",
                primary = true,
                modifier = Modifier.weight(1f),
                onClick = onStartRouting,
            )
            MediumButton(
                title = "Stop",
                primary = false,
                modifier = Modifier.weight(1f),
                onClick = onStopRouting,
            )
        }
        MediumButton(
            title = "Install Medium CA",
            primary = false,
            modifier = Modifier.fillMaxWidth(),
            onClick = onInstallCa,
        )
    }

    MediumCard {
        SectionTitle("Services")
        if (devices.isEmpty()) {
            EmptyServices()
        }
        devices.forEach { device ->
            Spacer(Modifier.height(14.dp))
            SectionTitle(device.name)
            device.services.forEach { service ->
                ServiceRow(service, onOpen = { onOpenService(service) })
            }
        }
    }
}

@androidx.compose.runtime.Composable
private fun MediumCard(content: @androidx.compose.runtime.Composable ColumnScope.() -> Unit) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .background(MediumPalette.Surface)
            .border(1.dp, MediumPalette.Stroke),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(1.dp)
                .background(Color.White.copy(alpha = 0.045f)),
        )
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
            content = content,
        )
    }
}

@androidx.compose.runtime.Composable
private fun FieldHeader(title: String, subtitle: String) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(title, color = MediumPalette.Ink, fontWeight = FontWeight.Bold, fontSize = 18.sp)
        Text(subtitle, color = MediumPalette.SecondaryText, fontSize = 12.sp, lineHeight = 17.sp)
    }
}

@androidx.compose.runtime.Composable
private fun SectionTitle(title: String) {
    Text(title, color = MediumPalette.Ink, fontWeight = FontWeight.Black, fontSize = 18.sp)
}

@androidx.compose.runtime.Composable
private fun InfoRow(title: String, value: String) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(title, color = MediumPalette.SecondaryText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
        Text(
            value,
            color = MediumPalette.Ink,
            fontFamily = FontFamily.Monospace,
            fontSize = 14.sp,
            lineHeight = 19.sp,
        )
    }
}

@androidx.compose.runtime.Composable
private fun StatusPill(text: String) {
    Text(
        text = text,
        color = MediumPalette.Ink,
        fontWeight = FontWeight.SemiBold,
        modifier = Modifier
            .fillMaxWidth()
            .background(MediumPalette.SurfaceStrong)
            .border(1.dp, MediumPalette.Stroke)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    )
}

@androidx.compose.runtime.Composable
private fun MediumInput(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    singleLine: Boolean = false,
    minHeight: androidx.compose.ui.unit.Dp = 52.dp,
    monospace: Boolean = false,
) {
    BasicTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = singleLine,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None),
        cursorBrush = SolidColor(MediumPalette.Accent),
        textStyle = TextStyle(
            color = MediumPalette.Ink,
            fontSize = 15.sp,
            fontFamily = if (monospace) FontFamily.Monospace else FontFamily.Default,
            fontWeight = FontWeight.SemiBold,
        ),
        modifier = Modifier
            .fillMaxWidth()
            .height(minHeight)
            .background(MediumPalette.Input)
            .border(1.dp, MediumPalette.InputStroke)
            .padding(13.dp),
        decorationBox = { innerTextField ->
            Box {
                if (value.isEmpty()) {
                    Text(
                        placeholder,
                        color = MediumPalette.MutedText,
                        fontFamily = if (monospace) FontFamily.Monospace else FontFamily.Default,
                    )
                }
                innerTextField()
            }
        },
    )
}

@androidx.compose.runtime.Composable
private fun MediumButton(
    title: String,
    primary: Boolean,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val background = when {
        !enabled -> MediumPalette.SurfaceStrong
        primary -> MediumPalette.Accent
        else -> MediumPalette.SurfaceStrong
    }
    val foreground = when {
        !enabled -> MediumPalette.MutedText
        primary -> Color.Black
        else -> MediumPalette.Ink
    }
    Box(
        modifier = modifier
            .height(50.dp)
            .background(background)
            .border(1.dp, if (primary && enabled) MediumPalette.Accent else MediumPalette.Stroke)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 14.dp),
        contentAlignment = androidx.compose.ui.Alignment.Center,
    ) {
        Text(title, color = foreground, fontWeight = FontWeight.Bold, fontSize = 15.sp)
    }
}

@androidx.compose.runtime.Composable
private fun EmptyServices() {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("No services loaded", color = MediumPalette.Ink, fontWeight = FontWeight.Bold)
        Text("Tap Refresh after joining the network.", color = MediumPalette.SecondaryText)
    }
}

@androidx.compose.runtime.Composable
private fun ServiceRow(service: PublishedService, onOpen: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 12.dp)
            .background(MediumPalette.SurfaceStrong)
            .border(1.dp, MediumPalette.Stroke)
            .clickable(enabled = service.kind != "ssh", onClick = onOpen)
            .padding(14.dp),
    ) {
        Box(
            modifier = Modifier
                .size(44.dp)
                .background(MediumPalette.Surface)
                .border(1.dp, MediumPalette.Stroke),
            contentAlignment = androidx.compose.ui.Alignment.Center,
        ) {
            Text(
                text = if (service.kind == "ssh") ">" else "O",
                color = MediumPalette.Accent,
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.Black,
            )
        }
        Spacer(Modifier.width(12.dp))
        Column(verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(service.displayName, color = MediumPalette.Ink, fontWeight = FontWeight.Bold)
            Text(
                service.kind.uppercase(),
                color = MediumPalette.Accent,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                letterSpacing = 1.sp,
            )
            Text(
                service.hostname,
                color = MediumPalette.SecondaryText,
                fontFamily = FontFamily.Monospace,
                fontSize = 12.sp,
                lineHeight = 16.sp,
            )
        }
    }
}

private object MediumPalette {
    val Background = Color(0xFF070809)
    val BackgroundUpper = Color(0xFF0E1012)
    val Ink = Color(0xFFF0F2F0)
    val SecondaryText = Color(0xFF949E9A)
    val MutedText = Color(0xFF666E6A)
    val Surface = Color(0xFF15181A)
    val SurfaceStrong = Color(0xFF1D2224)
    val Stroke = Color(0xFF383F40)
    val Input = Color(0xFF101316)
    val InputStroke = Color(0xFF424C49)
    val Accent = Color(0xFFB3DB2E)
}

internal fun mediumCaPemToDer(pem: String): ByteArray {
    val body = pem
        .replace("-----BEGIN CERTIFICATE-----", "")
        .replace("-----END CERTIFICATE-----", "")
        .lines()
        .joinToString(separator = "") { it.trim() }
    require(body.isNotBlank()) { "Medium CA response is empty" }
    return Base64.getMimeDecoder().decode(body)
}

private const val MEDIUM_CA_FILE_NAME = "medium-ca.crt"
private const val MEDIUM_CA_MIME_TYPE = "application/x-x509-ca-cert"

private sealed interface MediumCaInstallAction {
    data class InstallDirect(val der: ByteArray) : MediumCaInstallAction
    data object OpenSettings : MediumCaInstallAction
}
