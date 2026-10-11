package top.akadesk.app.controller

import android.os.Handler
import android.os.Looper
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONObject
import org.webrtc.DataChannel
import java.nio.ByteBuffer
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/** Bounded, correlated requests on an authenticated WebRTC peer. Mutations are never retried. */
class RemoteToolsChannel(private val sessionId:String,private val onReady:(Boolean)->Unit) {
    private val main=Handler(Looper.getMainLooper())
    private var channel:DataChannel?=null
    private var closed=false
    private var sequence=0L
    private data class Pending(val callback:(Result<JSONObject>)->Unit,val timeout:Runnable)
    private val pending=mutableMapOf<Long,Pending>()
    fun attach(value:DataChannel) {main.post {
        if(closed || channel!=null || value.label()!="tools") {value.close();value.dispose();return@post}
        channel=value
        value.registerObserver(object:DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount:Long)=Unit
            override fun onStateChange() {main.post {if(channel===value) {val ready=value.state()==DataChannel.State.OPEN;onReady(ready);if(!ready) failPending("工具通道已断开")}}}
            override fun onMessage(buffer:DataChannel.Buffer) {
                if(buffer.binary || buffer.data.remaining()>65_536) return
                val bytes=ByteArray(buffer.data.remaining()).also {buffer.data.get(it)}
                main.post {
                    if(closed || channel!==value) return@post
                    val response=runCatching {JSONObject(String(bytes,Charsets.UTF_8))}.getOrNull() ?: return@post
                    if(response.optString("session")!=sessionId) return@post
                    val request=pending.remove(response.optLong("id")) ?: return@post
                    main.removeCallbacks(request.timeout)
                    if(response.has("error")) request.callback(Result.failure(IllegalStateException(response.optString("error"))))
                    else request.callback(Result.success(response.optJSONObject("data") ?: JSONObject()))
                }
            }
        })
        onReady(value.state()==DataChannel.State.OPEN)
    }}
    suspend fun request(op:String,args:JSONObject=JSONObject()):JSONObject = suspendCancellableCoroutine {continuation->
        check(Looper.myLooper()==main.looper)
        val value=channel
        if(closed || value==null || value.state()!=DataChannel.State.OPEN || pending.size>=8 || value.bufferedAmount()>65_536) {
            continuation.resumeWithException(IllegalStateException("工具通道暂不可用"));return@suspendCancellableCoroutine
        }
        val id=++sequence
        val bytes=JSONObject().put("id",id).put("session",sessionId).put("op",op).put("args",args).toString().toByteArray()
        if(bytes.size>65_536) {continuation.resumeWithException(IllegalArgumentException("请求内容过长"));return@suspendCancellableCoroutine}
        val timeout=Runnable {pending.remove(id)?.callback?.invoke(Result.failure(IllegalStateException("未能确认操作结果，请检查后重试")))}
        pending[id]=Pending({result->if(continuation.isActive) result.fold(continuation::resume,continuation::resumeWithException)},timeout)
        main.postDelayed(timeout,15_000)
        continuation.invokeOnCancellation {main.post {pending.remove(id)?.let {main.removeCallbacks(it.timeout)}}}
        if(!value.send(DataChannel.Buffer(ByteBuffer.wrap(bytes),false))) {pending.remove(id);main.removeCallbacks(timeout);if(continuation.isActive) continuation.resumeWithException(IllegalStateException("请求发送失败"))}
    }
    private fun failPending(message:String) {val copy=pending.values.toList();pending.clear();copy.forEach {main.removeCallbacks(it.timeout);it.callback(Result.failure(IllegalStateException(message)))}}
    fun close() {
        if(Looper.myLooper()==main.looper) closeOnMain() else main.post {closeOnMain()}
    }
    private fun closeOnMain() {
        if(closed) return
        closed=true
        val closing=channel
        channel=null
        failPending("会话已结束")
        closing?.let {it.unregisterObserver();it.close();it.dispose()}
        onReady(false)
    }
}
