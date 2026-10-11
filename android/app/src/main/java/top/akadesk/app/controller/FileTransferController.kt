package top.akadesk.app.controller

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Base64
import androidx.compose.runtime.mutableStateListOf
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

data class FileTransferRecord(val id:String,val name:String,val receiving:Boolean,val size:Long,val completed:Long=0,val status:String="传输中",val time:Long=System.currentTimeMillis(),val error:String?=null)

class FileTransferController(private val context:Context,private val request:suspend (String,JSONObject)->JSONObject) {
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.Main.immediate)
    private val jobs=mutableMapOf<String,Job>()
    private val prefs=context.getSharedPreferences("akadesk-transfers",Context.MODE_PRIVATE)
    val records=mutableStateListOf<FileTransferRecord>()
    init {
        runCatching {
            val array=JSONArray(prefs.getString("history","[]"))
            for(index in 0 until array.length()) {
                val value=array.getJSONObject(index)
                records.add(FileTransferRecord(value.getString("id"),value.getString("name"),value.getBoolean("receiving"),value.getLong("size"),value.getLong("completed"),value.getString("status"),value.getLong("time")))
            }
        }
    }
    private fun update(id:String,change:(FileTransferRecord)->FileTransferRecord) {val index=records.indexOfFirst {it.id==id};if(index>=0) records[index]=change(records[index])}
    private fun saveHistory() {
        val array=JSONArray()
        records.filter {it.status!="传输中"}.takeLast(100).forEach {array.put(JSONObject().put("id",it.id).put("name",it.name).put("receiving",it.receiving).put("size",it.size).put("completed",it.completed).put("status",it.status).put("time",it.time))}
        prefs.edit().putString("history",array.toString()).apply()
    }
    fun clearHistory() {records.removeAll {it.status!="传输中"};saveHistory()}
    fun cancel(id:String) {jobs[id]?.cancel()}
    fun finishSession() {jobs.values.toList().forEach {it.cancel()}}
    fun upload(uri:Uri) {
        val id=UUID.randomUUID().toString()
        val info=runCatching {context.contentResolver.query(uri,arrayOf(OpenableColumns.DISPLAY_NAME,OpenableColumns.SIZE),null,null,null)?.use {cursor->
            if(cursor.moveToFirst()) cursor.getString(0) to (if(cursor.isNull(1)) -1L else cursor.getLong(1)) else null
        }}.getOrNull() ?: ("file-${System.currentTimeMillis()}" to -1L)
        if(jobs.size>=4) {
            records.add(FileTransferRecord(id,info.first,false,info.second.coerceAtLeast(0),status="传输失败",error="同时最多传输 4 个文件，请稍后重试"));saveHistory();return
        }
        records.add(FileTransferRecord(id,info.first,false,info.second.coerceAtLeast(0)))
        val job=scope.launch(start=CoroutineStart.LAZY) {
            var token:String?=null
            var temporary:File?=null
            try {
                var size=info.second
                if(size<0) {
                    temporary=File.createTempFile("akadesk-upload-",".tmp",context.cacheDir)
                    val file=temporary!!
                    size=withContext(Dispatchers.IO) {
                        context.contentResolver.openInputStream(uri)?.use {input->file.outputStream().use {output->
                            var total=0L;val bytes=ByteArray(16_384)
                            while(true) {ensureActive();val count=input.read(bytes);if(count<0) break;total+=count;require(total<=4L*1024*1024*1024) {"文件超过 4 GB"};output.write(bytes,0,count)}
                            total
                        }} ?: error("无法读取文件")
                    }
                }
                require(size<=4L*1024*1024*1024) {"文件超过 4 GB"}
                update(id) {it.copy(size=size)}
                token=request("upload_begin",JSONObject().put("name",info.first).put("size",size)).getString("transfer")
                val input=withContext(Dispatchers.IO) {temporary?.inputStream() ?: context.contentResolver.openInputStream(uri) ?: error("无法读取文件")}
                input.use {
                    var offset=0L
                    val bytes=ByteArray(16_384)
                    while(true) {
                        ensureActive()
                        val count=withContext(Dispatchers.IO) {input.read(bytes)}
                        if(count<0) break
                        if(count==0) continue
                        request("upload_chunk",JSONObject().put("transfer",token).put("offset",offset).put("bytes",Base64.encodeToString(bytes,0,count,Base64.NO_WRAP)))
                        offset+=count;update(id) {it.copy(completed=offset)}
                    }
                    require(offset==size) {"文件读取长度发生变化"}
                }
                request("upload_commit",JSONObject().put("transfer",token));token=null
                update(id) {it.copy(status="已发送",completed=size)}
            } catch(error:Exception) {update(id) {it.copy(status=if(error is CancellationException) "已取消" else "传输失败",error=error.message)}}
            finally {
                withContext(NonCancellable) {token?.let {runCatching {request("cancel",JSONObject().put("transfer",it))}}}
                temporary?.delete();jobs.remove(id);saveHistory()
            }
        }
        jobs[id]=job
        job.start()
    }
    fun download(path:String,name:String,destination:Uri) {
        if(jobs.size>=4) {
            records.add(FileTransferRecord(UUID.randomUUID().toString(),name,true,0,status="传输失败",error="同时最多传输 4 个文件，请稍后重试"));saveHistory()
            scope.launch(Dispatchers.IO) {runCatching {android.provider.DocumentsContract.deleteDocument(context.contentResolver,destination)}}
            return
        }
        val id=UUID.randomUUID().toString()
        records.add(FileTransferRecord(id,name,true,0))
        val job=scope.launch(start=CoroutineStart.LAZY) {
            var token:String?=null
            var successful=false
            try {
                val start=request("download_begin",JSONObject().put("path",path))
                token=start.getString("transfer")
                val size=start.getLong("size")
                update(id) {it.copy(size=size)}
                val output=withContext(Dispatchers.IO) {context.contentResolver.openOutputStream(destination,"w") ?: error("无法保存文件")}
                output.use {
                    var offset=0L
                    do {
                        ensureActive()
                        val chunk=request("download_chunk",JSONObject().put("transfer",token).put("offset",offset))
                        val bytes=Base64.decode(chunk.getString("bytes"),Base64.NO_WRAP)
                        require(bytes.size<=16_384 && offset+bytes.size<=size) {"收到无效文件数据"}
                        withContext(Dispatchers.IO) {output.write(bytes)}
                        offset+=bytes.size
                        require(chunk.getLong("offset")==offset) {"文件偏移不一致"}
                        update(id) {it.copy(completed=offset)}
                        if(chunk.getBoolean("done")) break
                        require(bytes.isNotEmpty()) {"收到空文件数据"}
                    } while(offset<size)
                    require(offset==size) {"文件传输不完整"}
                }
                token=null;successful=true;update(id) {it.copy(status="已接收",completed=size)}
            } catch(error:Exception) {update(id) {it.copy(status=if(error is CancellationException) "已取消" else "传输失败",error=error.message)}}
            finally {
                withContext(NonCancellable) {token?.let {runCatching {request("cancel",JSONObject().put("transfer",it))}}}
                if(!successful) withContext(Dispatchers.IO) {runCatching {android.provider.DocumentsContract.deleteDocument(context.contentResolver,destination)}}
                jobs.remove(id);saveHistory()
            }
        }
        jobs[id]=job
        job.start()
    }
}
