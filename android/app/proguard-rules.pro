-keep class org.webrtc.** { *; }
-keep class kotlinx.serialization.** { *; }
-keepclassmembers class com.opencloudgaming.opennow.** {
    @kotlinx.serialization.Serializable *;
}
