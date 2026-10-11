package top.akadesk.app.controller

import org.junit.Assert.assertEquals
import org.junit.Test

/** Method signatures used by google-webrtc:1.0.32006's native audio module. */
class MediaPlayoutJniContractTest {
    @Test fun mediaAdapterRetainsEveryPinnedNativeEntryPoint() {
        val type=Class.forName("org.webrtc.audio.WebRtcAudioTrack")
        fun check(name:String,result:Class<*>,vararg args:Class<*>)=assertEquals(result,type.getDeclaredMethod(name,*args).returnType)
        check("initPlayout",Integer.TYPE,Integer.TYPE,Integer.TYPE,java.lang.Double.TYPE)
        for(name in listOf("getBufferSizeInFrames","getStreamMaxVolume","getStreamVolume","GetPlayoutUnderrunCount")) check(name,Integer.TYPE)
        for(name in listOf("startPlayout","stopPlayout")) check(name,java.lang.Boolean.TYPE)
        check("setStreamVolume",java.lang.Boolean.TYPE,Integer.TYPE)
        check("setNativeAudioTrack",java.lang.Void.TYPE,java.lang.Long.TYPE)
        check("nativeCacheDirectBufferAddress",java.lang.Void.TYPE,java.lang.Long.TYPE,java.nio.ByteBuffer::class.java)
        check("nativeGetPlayoutData",java.lang.Void.TYPE,java.lang.Long.TYPE,Integer.TYPE)
    }
}
