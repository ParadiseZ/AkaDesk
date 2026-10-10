package top.akadesk.app.controller

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Handler
import android.os.Looper

/** Own only temporary routing/focus; never change the user's system volume. */
@Suppress("DEPRECATION")
internal class RemoteAudioOutput(context: Context, private val onFocus: (Boolean) -> Unit) {
    private val manager = context.getSystemService(AudioManager::class.java)
    private var originalMode: Int? = null
    private var originalSpeaker = false
    private var speakerChanged = false
    private var active = false
    private val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
        .setAudioAttributes(AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
        .setOnAudioFocusChangeListener({ change ->
            if (active) onFocus(change == AudioManager.AUDIOFOCUS_GAIN)
        }, Handler(Looper.getMainLooper()))
        .build()

    fun start() {
        if (active) return
        active = true
        originalMode = manager.mode
        originalSpeaker = manager.isSpeakerphoneOn
        manager.mode = AudioManager.MODE_IN_COMMUNICATION
        val externalOutput = manager.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any {
            it.type in setOf(AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
                AudioDeviceInfo.TYPE_BLUETOOTH_A2DP, AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
                AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_USB_DEVICE)
        }
        if (!externalOutput && !originalSpeaker) {
            manager.isSpeakerphoneOn = true
            speakerChanged = true
        }
        requestFocus()
    }

    fun requestFocus() {
        if (active) onFocus(manager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
    }

    fun close() {
        if (!active) return
        active = false
        manager.abandonAudioFocusRequest(request)
        if (speakerChanged) manager.isSpeakerphoneOn = originalSpeaker
        originalMode?.let { if (manager.mode == AudioManager.MODE_IN_COMMUNICATION) manager.mode = it }
        originalMode = null
    }
}
