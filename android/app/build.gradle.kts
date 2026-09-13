plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.contextswitch"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.contextswitch"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    buildFeatures {
        compose = true
    }

    lint {
        textReport = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

// Build the Rust core (cargo-ndk) and generate the UniFFI Kotlin bindings.
// Requires: ANDROID_NDK_HOME (or sdk/ndk via ANDROID_HOME), cargo-ndk,
// and the rustup Android targets. See android/README.md.
tasks.register<Exec>("cargoNdkBuild") {
    workingDir = rootDir
    commandLine("bash", "build-rust.sh")
}
tasks.named("preBuild") { dependsOn("cargoNdkBuild") }

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.08.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.12.0")
    implementation("androidx.navigation:navigation-compose:2.9.4")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.9.4")
    implementation("androidx.core:core-ktx:1.17.0")
    // UniFFI-generated Kotlin bindings load the Rust cdylib through JNA.
    implementation("net.java.dev.jna:jna:5.15.0@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
