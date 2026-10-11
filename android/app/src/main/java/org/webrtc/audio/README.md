# Media playout adapter

Source: WebRTC `refs/branch-heads/4103`,
`sdk/android/src/java/org/webrtc/audio/WebRtcAudioTrack.java`.
https://webrtc.googlesource.com/src/+/refs/branch-heads/4103/sdk/android/src/java/org/webrtc/audio/WebRtcAudioTrack.java

The application pins `org.webrtc:google-webrtc:1.0.32006`. Its Java audio
device module has no media AudioAttributes option. The build strips only
the original WebRtcAudioTrack classes from the AAR and compiles this source.
Native libraries and other Java classes remain unchanged.

Changes: media/music AudioAttributes and STREAM_MUSIC; refuse global stream
volume writes; use AndroidX Nullable; adapt initPlayout to the pinned native
JNI contract `(IID)I` (returns minimum buffer bytes, or -1 on failure).
Retain the pinned JNI `getBufferSizeInFrames()I` accessor.
The native callbacks, class names, fields and constructor remain compatible.
Upgrading the AAR requires revalidation; Gradle checks the resolved version.

LICENSE, PATENTS and AUTHORS are copied from the same upstream branch.
