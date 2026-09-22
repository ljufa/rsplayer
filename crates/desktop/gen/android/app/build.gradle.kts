import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

// The version is bumped by hand on every release, together with the workspace version in
// Cargo.toml and a fastlane changelog named <versionCode>.txt. The versionCode/versionName
// literals in defaultConfig are literals on purpose: F-Droid's update checker reads them
// with a regex and never runs Gradle code. versionCode is the base
// major * 1_000_000 + minor * 1_000 + patch, the same scheme Tauri uses.
val workspaceVersion: String = rootProject.file("../../../../Cargo.toml").readLines()
    .dropWhile { it.trim() != "[workspace.package]" }
    .firstNotNullOf { Regex("""^version\s*=\s*"([^"]+)"""").find(it)?.groupValues?.get(1) }
val workspaceVersionCode: Int = workspaceVersion.split("-")[0].split(".").map { it.toInt() }
    .let { (major, minor, patch) -> major * 1_000_000 + minor * 1_000 + patch }

// Upload key for Play / release: gen/android/keystore.properties (git-ignored) with
// password, keyAlias, storeFile — see https://tauri.app/distribute/sign/android/.
// Without it release builds are signed with the debug key so they still install.
val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties().apply {
    if (keystorePropertiesFile.exists()) {
        keystorePropertiesFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "de.rsplayer.app"
    // NDK r28+ produces 16 KB page-aligned libraries by default (Play requirement).
    ndkVersion = "28.2.13676358"
    // AGP otherwise embeds Google-encrypted dependency metadata in the APK signing block,
    // which F-Droid's scanner rejects ("extra signing block 'Dependency metadata'").
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }
    defaultConfig {
        // The webview loads the in-process backend over http://localhost.
        manifestPlaceholders["usesCleartextTraffic"] = "true"
        applicationId = "de.rsplayer.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 5000000
        versionName = "5.0.0"
    }
    signingConfigs {
        if (keystorePropertiesFile.exists()) {
            create("release") {
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["password"] as String
                storeFile = file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["password"] as String
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = if (keystorePropertiesFile.exists()) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

// Fail the build if the literals in defaultConfig drift from the workspace version.
check(android.defaultConfig.versionName == workspaceVersion &&
        android.defaultConfig.versionCode == workspaceVersionCode) {
    "app/build.gradle.kts has ${android.defaultConfig.versionName} " +
        "(${android.defaultConfig.versionCode}) but Cargo.toml has " +
        "$workspaceVersion ($workspaceVersionCode): update versionName/versionCode"
}

// One APK per ABI for F-Droid: the recipe builds each ABI on its own and exports
// RSPLAYER_ABI_CODE (armeabi-v7a=1, arm64-v8a=2, x86=3, x86_64=4). The APK versionCode is
// then 10 * base + that digit, so a newer release always outranks every ABI of an older one
// (https://f-droid.org/docs/Submitting_to_F-Droid_Quick_Start_Guide/#setup-abi-split).
// Without the variable (universal APK, local builds) the digit is 0.
androidComponents {
    onVariants { variant ->
        val abiCode = System.getenv("RSPLAYER_ABI_CODE")?.toInt() ?: 0
        val base = android.defaultConfig.versionCode!!
        variant.outputs.forEach { it.versionCode.set(base * 10 + abiCode) }
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // Media session / notification / lock-screen controls + WebSocket to the backend
    implementation("androidx.media3:media3-session:1.8.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")