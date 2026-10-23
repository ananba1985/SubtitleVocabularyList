package com.subtitlevocabularylist.android

import android.app.Activity
import android.net.Uri
import org.json.JSONArray
import app.tauri.plugin.JSObject
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Container chapter metadata only; playback and learning rules remain in Rust. */
internal object MediaChapters {
    fun read(activity: Activity, uri: Uri): JSONArray {
        val result=JSONArray()
        try { activity.contentResolver.openFileDescriptor(uri,"r")?.use { descriptor ->
            java.io.FileInputStream(descriptor.fileDescriptor).use { input ->
                val channel=input.channel
                fun bytes(position:Long,count:Int):ByteArray {
                    require(count in 0..1048576 && position>=0)
                    channel.position(position);val out=ByteBuffer.allocate(count)
                    while(out.hasRemaining())if(channel.read(out)<0)throw java.io.EOFException()
                    return out.array()
                }
                fun number(data:ByteArray):Long {var n=0L;for(b in data)n=(n shl 8) or (b.toLong() and 255);return n}
                fun chapter(title:String,time:Long){if(result.length()<1000)result.put(JSObject().put("title",title.ifBlank{"章节 ${result.length()+1}"}).put("positionMs",time.coerceAtLeast(0)))}
                // Nero MP4 chapter list, including udta nested inside moov.
                fun mp4(start:Long,end:Long,depth:Int){
                    if(depth>5)return
                    var cursor=start;var count=0
                    while(cursor+8<=end && count++<100000){
                        val head=bytes(cursor,8);var size=number(head.copyOfRange(0,4));val type=String(head,4,4,Charsets.US_ASCII);var header=8L
                        if(size==1L){size=number(bytes(cursor+8,8));header=16}
                        if(size==0L)size=end-cursor
                        if(size<header || size>end-cursor)return
                        if(type=="moov"||type=="udta")mp4(cursor+header,cursor+size,depth+1)
                        if(type=="chpl" && size-header in 9..1048576){
                            val raw=bytes(cursor+header,(size-header).toInt());var i=8;val n=raw[i++].toInt() and 255
                            repeat(n){if(i+9<=raw.size){val time=number(raw.copyOfRange(i,i+8))/10000;i+=8;val len=raw[i++].toInt() and 255;if(i+len<=raw.size){chapter(String(raw,i,len,Charsets.UTF_8),time);i+=len}}}
                        }
                        cursor+=size
                    }
                }
                // Matroska Chapters/EditionEntry/ChapterAtom. Skip video clusters by their declared size.
                data class Element(val id:Long,val start:Long,val end:Long)
                fun element(position:Long,end:Long):Element? {
                    if(position>=end)return null
                    fun vint(p:Long,keep:Boolean):Pair<Long,Int>{val first=bytes(p,1)[0].toInt() and 255;require(first>0);var len=1;var mask=128;while(first and mask==0){len++;mask=mask shr 1};require(len<=8);val data=bytes(p,len);var n=if(keep)first.toLong() else (first and (mask-1)).toLong();for(i in 1 until len)n=(n shl 8) or (data[i].toLong() and 255);return n to len}
                    val(id,il)=vint(position,true);val(size,sl)=vint(position+il,false);val start=position+il+sl
                    val unknown=size==((1L shl (7*sl))-1)
                    return Element(id,start,if(unknown)end else (start+size).coerceAtMost(end))
                }
                fun mkv(start:Long,end:Long,depth:Int){
                    if(depth>7)return
                    var cursor=start;var count=0
                    while(cursor<end && count++<100000){
                        val e=element(cursor,end)?:break;if(e.end<=cursor)break
                        if(e.id==0x18538067L || e.id==0x1043A770L || e.id==0x45B9L)mkv(e.start,e.end,depth+1)
                        if(e.id==0xB6L && e.end-e.start<=1048576){
                            var pos=e.start;var time=0L;var title=""
                            while(pos<e.end){val c=element(pos,e.end)?:break;if(c.id==0x91L && c.end-c.start<=8)time=number(bytes(c.start,(c.end-c.start).toInt()))/1000000
                                if(c.id==0x80L){var d=c.start;while(d<c.end){val name=element(d,c.end)?:break;if(name.id==0x85L && name.end-name.start<=4000)title=String(bytes(name.start,(name.end-name.start).toInt()),Charsets.UTF_8);d=name.end}}
                                pos=c.end}
                            chapter(title,time)
                        }
                        cursor=e.end
                    }
                }
                if(number(bytes(0,4))==0x1A45DFA3L)mkv(0,channel.size(),0) else mp4(0,channel.size(),0)
            }
        }} catch (_:Exception) { /* Unsupported or non-seekable source has no advertised chapters. */ }
        return result
    }
}
