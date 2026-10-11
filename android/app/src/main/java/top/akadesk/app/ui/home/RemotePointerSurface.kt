@file:OptIn(androidx.compose.ui.ExperimentalComposeUiApi::class)
package top.akadesk.app.ui.home

import android.view.MotionEvent
import androidx.compose.foundation.*
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.pointerInteropFilter
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.*
import androidx.compose.ui.viewinterop.AndroidView
import org.webrtc.SurfaceViewRenderer
import top.akadesk.app.controller.*
import kotlin.math.*

@Composable
internal fun RemoteVideoSurface(c:ControllerCoordinator,s:ActiveRemoteSession,enabled:Boolean,onReveal:()->Unit,rightModeEnabled:Boolean) {
    var size by remember {mutableStateOf(IntSize.Zero)}
    var zoom by rememberSaveable(s.sessionId) {mutableFloatStateOf(1f)}
    var pan by remember {mutableStateOf(Offset.Zero)}
    var right by remember {mutableStateOf(false)}
    var down by remember {mutableStateOf(Offset.Zero)}
    var last by remember {mutableStateOf(Offset.Zero)}
    var distance by remember {mutableFloatStateOf(0f)}
    var moved by remember {mutableStateOf(false)}
    var multi by remember {mutableStateOf(false)}
    var dragging by remember {mutableStateOf(false)}
    val trackpad=c.remotePreferences.pointerMode==PointerMode.Trackpad
    LaunchedEffect(enabled,trackpad,c.remotePreferences.tabletTouch) {c.releaseRemoteInput();dragging=false;multi=false;right=false}
    BoxWithConstraints(Modifier.fillMaxSize().clip(androidx.compose.ui.graphics.RectangleShape),contentAlignment=Alignment.Center) {
        val aspect=c.activeVideoAspectRatio
        val width=minOf(maxWidth,maxHeight*aspect)
        val height=width/aspect
        val density=LocalDensity.current
        val widthPx=with(density){width.toPx()};val heightPx=with(density){height.toPx()}
        val origin=Offset((size.width-widthPx)/2,(size.height-heightPx)/2)
        fun target(p:Offset,clamp:Boolean=false):Offset? {
            val x=((p.x-origin.x-widthPx/2-pan.x)/zoom+widthPx/2)/widthPx
            val y=((p.y-origin.y-heightPx/2-pan.y)/zoom+heightPx/2)/heightPx
            if(!x.isFinite() || !y.isFinite() || !clamp && (x !in 0f..1f || y !in 0f..1f)) return null
            return Offset(x.coerceIn(0f,1f),y.coerceIn(0f,1f))
        }
        fun point(p:Offset,pressed:Boolean) {
            val t=target(p) ?: return
            if(right) {c.pointerInput(t.x,t.y,false,true);c.pointerButton(true,pressed)} else c.pointerInput(t.x,t.y,pressed)
        }
        key(s.sessionId ?: s.host.deviceId) {
            AndroidView(modifier=Modifier.width(width).height(height).graphicsLayer {scaleX=zoom;scaleY=zoom;translationX=pan.x;translationY=pan.y},
                factory={context->SurfaceViewRenderer(context).also(c::bindActiveVideoRenderer)},update=c::bindActiveVideoRenderer,onReset=null,onRelease=c::releaseActiveVideoRenderer)
        }
        Box(Modifier.fillMaxSize().onSizeChanged {size=it}.testTag("remote-pointer")
            .pointerInput(enabled,widthPx,heightPx,zoom,pan) {
                awaitPointerEventScope {
                    while(true) {
                        val event=awaitPointerEvent()
                        val mouse=event.changes.firstOrNull {it.type==PointerType.Mouse}
                        if(enabled && mouse!=null) {
                            if(event.type==PointerEventType.Move) target(mouse.position)?.let {c.pointerInput(it.x,it.y,false,true)}
                            if(event.type==PointerEventType.Scroll) {
                                c.scrollInput((-mouse.scrollDelta.x).coerceIn(-10f,10f),(-mouse.scrollDelta.y).coerceIn(-10f,10f))
                                mouse.consume()
                            }
                        }
                    }
                }
            }.pointerInteropFilter {event->
            if(!enabled) false else {
                val p=Offset(event.x,event.y)
                if(event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)) {
                    target(p)?.let {c.pointerInput(it.x,it.y,false,true)}
                    when(event.actionMasked) {
                        MotionEvent.ACTION_DOWN->c.pointerButton(event.buttonState and MotionEvent.BUTTON_SECONDARY!=0,true)
                        MotionEvent.ACTION_UP,MotionEvent.ACTION_CANCEL->c.releaseRemoteInput()
                        MotionEvent.ACTION_BUTTON_PRESS->c.pointerButton(event.actionButton==MotionEvent.BUTTON_SECONDARY,true)
                        MotionEvent.ACTION_BUTTON_RELEASE->c.pointerButton(event.actionButton==MotionEvent.BUTTON_SECONDARY,false)
                    }
                    return@pointerInteropFilter true
                }
                if(c.remotePreferences.tabletTouch && !trackpad && !event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)) {
                    if(event.actionMasked==MotionEvent.ACTION_DOWN && target(p)==null) return@pointerInteropFilter false
                    if(event.actionMasked==MotionEvent.ACTION_CANCEL) c.releaseRemoteInput()
                    else if(event.actionMasked in listOf(MotionEvent.ACTION_DOWN,MotionEvent.ACTION_POINTER_DOWN,MotionEvent.ACTION_MOVE,MotionEvent.ACTION_UP,MotionEvent.ACTION_POINTER_UP)) {
                        val contacts=(0 until minOf(event.pointerCount,10)).map {index->
                            val t=target(Offset(event.getX(index),event.getY(index)),clamp=true) ?: return@pointerInteropFilter false
                            val changed=index==event.actionIndex
                            val phase=when {changed && event.actionMasked in listOf(MotionEvent.ACTION_DOWN,MotionEvent.ACTION_POINTER_DOWN)->1;changed && event.actionMasked in listOf(MotionEvent.ACTION_UP,MotionEvent.ACTION_POINTER_UP)->3;else->2}
                            akadesk.v1.TouchPoint.newBuilder().setId(event.getPointerId(index)).setNormalizedX(t.x).setNormalizedY(t.y).setPhase(phase).build()
                        }
                        c.touchInput(contacts)
                    }
                    return@pointerInteropFilter true
                }
                when(event.actionMasked) {
                    MotionEvent.ACTION_DOWN->{down=p;last=p;moved=false;multi=false;dragging=false;distance=0f;if(!trackpad && target(p)!=null) {point(p,true);dragging=true}}
                    MotionEvent.ACTION_POINTER_DOWN->{multi=true;dragging=false;c.releaseRemoteInput();distance=if(event.pointerCount>=2) hypot(event.getX(0)-event.getX(1),event.getY(0)-event.getY(1)) else 0f;last=if(event.pointerCount>=2) Offset((event.getX(0)+event.getX(1))/2,(event.getY(0)+event.getY(1))/2) else p}
                    MotionEvent.ACTION_MOVE->{
                        if(event.pointerCount>=2) {
                            val center=Offset((event.getX(0)+event.getX(1))/2,(event.getY(0)+event.getY(1))/2)
                            val span=hypot(event.getX(0)-event.getX(1),event.getY(0)-event.getY(1))
                            if(distance>0 && abs(span-distance)>2f) {zoom=(zoom*span/distance).coerceIn(1f,4f);if(zoom==1f) pan=Offset.Zero}
                            else if(trackpad) c.scrollInput(((last.x-center.x)/60).coerceIn(-10f,10f),((last.y-center.y)/60).coerceIn(-10f,10f))
                            else if(zoom>1) pan+=center-last
                            pan=Offset(pan.x.coerceIn(-widthPx*(zoom-1)/2,widthPx*(zoom-1)/2),pan.y.coerceIn(-heightPx*(zoom-1)/2,heightPx*(zoom-1)/2))
                            last=center;distance=span
                        } else if(!multi) {
                            if(trackpad) {
                                if(!moved && event.eventTime-event.downTime>350 && !dragging) {c.pointerButton(right,true);dragging=true}
                                c.movePointer((p.x-last.x)/widthPx,(p.y-last.y)/heightPx,dragging && !right)
                            } else if(dragging) point(p,true)
                            if((p-down).getDistance()>8) moved=true
                            last=p
                        }
                    }
                    MotionEvent.ACTION_UP->{if(dragging) {if(trackpad) c.pointerButton(right,false) else {point(p,false);c.releaseRemoteInput()}} else if(trackpad && !multi && !moved) {c.pointerButton(right,true);c.pointerButton(right,false)} else if(!trackpad && !multi && target(p)==null) onReveal();dragging=false}
                    MotionEvent.ACTION_CANCEL->{c.releaseRemoteInput();dragging=false}
                    MotionEvent.ACTION_POINTER_UP->{c.releaseRemoteInput();dragging=false}
                    MotionEvent.ACTION_SCROLL->c.scrollInput(event.getAxisValue(MotionEvent.AXIS_HSCROLL).coerceIn(-10f,10f),event.getAxisValue(MotionEvent.AXIS_VSCROLL).coerceIn(-10f,10f))
                }
                true
            }
        })
        if(enabled && rightModeEnabled) Box(Modifier.align(Alignment.CenterStart).width(36.dp).fillMaxHeight(.32f).background(if(right) Color(0x99477CFF) else Color(0x33000000))
            .pointerInput(c) {detectTapGestures(onPress={right=true;c.releaseRemoteInput();tryAwaitRelease();right=false;c.releaseRemoteInput()})},contentAlignment=Alignment.Center) {Text("右键",color=Color.White,style=MaterialTheme.typography.labelSmall)}
    }
}

@Composable
internal fun MappingOverlay(c:ControllerCoordinator,enabled:Boolean,editing:Boolean) {
    var area by remember {mutableStateOf(IntSize.Zero)}
    val density=LocalDensity.current
    val button=c.remotePreferences.buttonSize.dp
    val buttonPx=with(density) {button.toPx()}
    Box(Modifier.fillMaxSize().onSizeChanged {area=it}) {
        c.inputShortcuts.forEachIndexed {index,key->
            val saved=remember(key.id,c.selectedInputProfile) {c.mappingPosition(key.id)}
            var position by remember(key.id,c.selectedInputProfile) {mutableStateOf(Offset(if(saved.first<0) (if(index%2==0) .04f else .90f) else saved.first,if(saved.second<0) .08f+(index/2)*.12f else saved.second))}
            val availableW=(area.width-buttonPx).coerceAtLeast(1f);val availableH=(area.height-buttonPx).coerceAtLeast(1f)
            Box(Modifier.offset {IntOffset((position.x.coerceIn(0f,1f)*availableW).toInt(),(position.y.coerceIn(0f,1f)*availableH).toInt())}.size(button).clip(CircleShape)
                .background(Color(0xFF303744).copy(alpha=c.remotePreferences.buttonOpacity)).border(1.dp,if(editing) Color(0xFF477CFF) else Color.LightGray,CircleShape)
                .pointerInput(editing,enabled,key,c.selectedInputProfile,area) {
                    if(editing) detectDragGestures(onDragEnd={c.saveMappingPosition(key.id,position.x,position.y)}) {change,drag->change.consume();position=Offset((position.x+drag.x/availableW).coerceIn(0f,1f),(position.y+drag.y/availableH).coerceIn(0f,1f))}
                    else if(enabled) detectTapGestures(onPress={c.shortcutKey(key,true);try {tryAwaitRelease()} finally {c.shortcutKey(key,false)}})
                },contentAlignment=Alignment.Center) {Text(key.label,color=Color.White,style=MaterialTheme.typography.labelSmall,maxLines=2)}
        }
    }
}

@Composable
internal fun VirtualMouse(c:ControllerCoordinator,enabled:Boolean) {
    Row(Modifier.fillMaxSize().wrapContentSize(Alignment.BottomCenter).padding(bottom=24.dp),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        listOf(false to "左键",true to "右键").forEach {(right,title)->
            Box(Modifier.size(48.dp).clip(CircleShape).background(Color(0x99303744)).pointerInput(enabled,right) {if(enabled) detectTapGestures(onPress={c.pointerButton(right,true);try {tryAwaitRelease()} finally {c.pointerButton(right,false)}})},contentAlignment=Alignment.Center) {Text(title,color=Color.White,style=MaterialTheme.typography.labelSmall)}
        }
        TextButton(onClick={c.scrollInput(0f,1f)},enabled=enabled) {Text("滚轮 ↑")}
        TextButton(onClick={c.scrollInput(0f,-1f)},enabled=enabled) {Text("滚轮 ↓")}
    }
}
