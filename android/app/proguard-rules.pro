-keep class com.sun.jna.** { *; }
-keep class dev.contextswitch.** { *; }
# JNA references desktop-only AWT classes that don't exist on Android.
-dontwarn java.awt.**
