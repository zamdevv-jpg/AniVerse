import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
val signingProperties = Properties().apply {
    val propFile = file("signing.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "com.otakunime.desktop"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "true"
        applicationId = "com.otakunime.desktop"
        minSdk = 24
        targetSdk = 35
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        create("release") {
            val keystorePath = providers.environmentVariable("OTAKUNIME_ANDROID_KEYSTORE").orNull
                ?: signingProperties.getProperty("storeFile")
            val storeSecret = providers.environmentVariable("OTAKUNIME_ANDROID_STORE_PASSWORD").orNull
                ?: signingProperties.getProperty("storePassword")
            val alias = providers.environmentVariable("OTAKUNIME_ANDROID_KEY_ALIAS").orNull
                ?: signingProperties.getProperty("keyAlias")
            val keySecret = providers.environmentVariable("OTAKUNIME_ANDROID_KEY_PASSWORD").orNull
                ?: signingProperties.getProperty("keyPassword")
            val releaseRequested = gradle.startParameter.taskNames.any {
                it.contains("release", ignoreCase = true)
            }

            if (releaseRequested) {
                require(!keystorePath.isNullOrBlank() && !storeSecret.isNullOrBlank() &&
                    !alias.isNullOrBlank() && !keySecret.isNullOrBlank()) {
                    "Release signing is not configured. Set OTAKUNIME_ANDROID_KEYSTORE, " +
                        "OTAKUNIME_ANDROID_STORE_PASSWORD, OTAKUNIME_ANDROID_KEY_ALIAS, and " +
                    "OTAKUNIME_ANDROID_KEY_PASSWORD in the build environment or provide " +
                        "app/signing.properties (see README)."
                }
                val keystoreFile = file(keystorePath!!)
                require(keystoreFile.isFile) { "Android release keystore not found: $keystoreFile" }
                storeFile = keystoreFile
                storePassword = storeSecret
                keyAlias = alias
                keyPassword = keySecret
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isMinifyEnabled = false
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
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
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
