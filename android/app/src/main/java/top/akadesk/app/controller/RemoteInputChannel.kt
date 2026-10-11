package top.akadesk.app.controller

import akadesk.v1.*
import android.os.Handler
import android.os.Looper
import org.webrtc.DataChannel
import java.nio.ByteBuffer

/** Reliable ordered protobuf input; the peer owns authentication and binds the session. */
class RemoteInputChannel(
    private val sessionId: String,
    private val onReady: (Boolean) -> Unit,
    private val onResult: (InputResult) -> Unit,
    private val onError: () -> Unit,
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var channel: DataChannel? = null
    private var seq = 0L
    private var stopped = false

    // Native observers run on WebRTC's signaling thread. They only copy/post data:
    // a monitor held across a JNI call can deadlock against that thread's callback.
    fun attach(value: DataChannel) {
        mainHandler.post { attachOnMain(value) }
    }

    private fun attachOnMain(value: DataChannel) {
        if (stopped || channel != null || value.label() != "input") { value.close(); value.dispose(); return }
        channel = value
        value.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                mainHandler.post {
                    if (channel === value && !stopped) onReady(value.state() == DataChannel.State.OPEN)
                }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                if (!buffer.binary || buffer.data.remaining() > 16_384) return
                val bytes = ByteArray(buffer.data.remaining()).also { buffer.data.get(it) }
                mainHandler.post {
                    if (stopped || channel !== value) return@post
                    val result = runCatching { InputResult.parseFrom(bytes) }.getOrNull() ?: return@post
                    if (result.sessionId == sessionId && result.seq in 1..seq) {
                        onResult(result)
                        if (result.hasError()) { onError(); closeOnMain() }
                    }
                }
            }
        })
        onReady(value.state() == DataChannel.State.OPEN)
    }
    private fun send(build: (Long) -> InputEvent.Builder): Boolean {
        check(Looper.myLooper() == mainHandler.looper) { "Input must be sent from the main thread" }
        val dc = channel ?: return false
        if (stopped || dc.state() != DataChannel.State.OPEN) return false
        // Bound latency: close on congestion so the host releases held input.
        if (dc.bufferedAmount() > 65_536) { onError(); close(); return false }
        val event = build(++seq).setSessionId(sessionId).build()
        if (!dc.send(DataChannel.Buffer(ByteBuffer.wrap(event.toByteArray()), true))) {
            onError(); close(); return false
        }
        return true
    }
    fun pointer(x: Float, y: Float, button: PointerButton = PointerButton.POINTER_BUTTON_UNSPECIFIED, pressed: Boolean = false): Boolean {
        if (!x.isFinite() || !y.isFinite() || x !in 0f..1f || y !in 0f..1f) return false
        return send { InputEvent.newBuilder().setPointer(PointerEvent.newBuilder().setSeq(it)
            .setNormalizedX(x).setNormalizedY(y).setButton(button).setPressed(pressed).setTimestampMs(System.currentTimeMillis())) }
    }
    fun scroll(x: Float, y: Float): Boolean {
        if (!x.isFinite() || !y.isFinite() || kotlin.math.abs(x)>10 || kotlin.math.abs(y)>10) return false
        return send { InputEvent.newBuilder().setScroll(ScrollEvent.newBuilder().setSeq(it).setDeltaX(x).setDeltaY(y).setTimestampMs(System.currentTimeMillis())) }
    }
    fun touch(points: List<TouchPoint>): Boolean {
        if(points.isEmpty() || points.size>10) return false
        return send { InputEvent.newBuilder().setTouch(TouchInputEvent.newBuilder().setSeq(it)
            .addAllPoints(points).setTimestampMs(System.currentTimeMillis())) }
    }
    fun key(code: Int, down: Boolean, ctrl: Boolean = false, alt: Boolean = false, shift: Boolean = false, meta: Boolean = false): Boolean {
        if (code !in 8..254) return false
        return send { InputEvent.newBuilder().setKey(KeyEvent.newBuilder().setSeq(it).setKeyCode(code)
            .setAction(if (down) KeyAction.KEY_ACTION_DOWN else KeyAction.KEY_ACTION_UP)
            .setCtrl(ctrl).setAlt(alt).setShift(shift).setMeta(meta).setTimestampMs(System.currentTimeMillis())) }
    }
    fun text(text: String): Boolean {
        if (text.isEmpty() || text.toByteArray(Charsets.UTF_8).size > 4096 || '\u0000' in text) return false
        return send { InputEvent.newBuilder().setText(TextInputEvent.newBuilder().setSeq(it).setText(text).setTimestampMs(System.currentTimeMillis())) }
    }
    fun release(): Boolean = send { InputEvent.newBuilder().setRelease(ReleaseInputEvent.newBuilder().setSeq(it).setTimestampMs(System.currentTimeMillis())) }
    fun close() {
        if (Looper.myLooper() == mainHandler.looper) closeOnMain()
        else mainHandler.post { closeOnMain() }
    }

    private fun closeOnMain() {
        if (stopped) return
        stopped = true
        val closing = channel
        channel = null
        closing?.let {
            if (it.state() == DataChannel.State.OPEN && it.bufferedAmount() <= 65_536) {
                val event = InputEvent.newBuilder().setSessionId(sessionId)
                    .setRelease(ReleaseInputEvent.newBuilder().setSeq(++seq).setTimestampMs(System.currentTimeMillis())).build()
                it.send(DataChannel.Buffer(ByteBuffer.wrap(event.toByteArray()), true))
            }
            it.unregisterObserver(); it.close(); it.dispose()
        }
        onReady(false)
    }
}
