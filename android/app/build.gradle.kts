import java.util.zip.ZipInputStream
import java.util.zip.ZipOutputStream
import java.util.zip.ZipEntry
import java.io.ByteArrayOutputStream
import java.io.ByteArrayInputStream
import com.google.protobuf.gradle.proto

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.google.protobuf)
}

android {
    namespace = "top.akadesk.app"
    compileSdk {
        version = release(36) {
            minorApiLevel = 1
        }
    }

    defaultConfig {
        applicationId = "top.akadesk.app"
        minSdk = 28
        targetSdk = 37
        versionCode = 1
        versionName = "1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
    buildFeatures {
        compose = true
    }
    buildToolsVersion = "37.0.0"

    sourceSets {
        getByName("main") {
            java.directories.addAll(
                listOf("src/main/java")
            )
            proto {
                srcDir("../../proto")
            }
        }
    }
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:${libs.versions.protobufLite.get()}"
    }
    generateProtoTasks {
        all().configureEach {
            builtins {
                create("java") {
                    option("lite")
                }
            }
        }
    }
}

// The pinned WebRTC AAR has no media AudioAttributes option. Replace only its
// Java playout class; native libraries, JNI names and the remaining API stay intact.
val originalWebRtc by configurations.creating { isTransitive = false }
dependencies { originalWebRtc(libs.google.webrtc) }
val mediaWebRtcAar = layout.buildDirectory.file("generated/webrtc/google-webrtc-media.aar")
val prepareMediaWebRtc by tasks.registering {
    inputs.files(originalWebRtc)
    inputs.file("src/main/java/org/webrtc/audio/WebRtcAudioTrack.java")
    outputs.file(mediaWebRtcAar)
    doLast {
        check(originalWebRtc.resolvedConfiguration.resolvedArtifacts.single().moduleVersion.id.version == "1.0.32006") {
            "The media playout JNI adapter must be revalidated when upgrading WebRTC"
        }
        val target = mediaWebRtcAar.get().asFile
        target.parentFile.mkdirs()
        ZipInputStream(originalWebRtc.singleFile.inputStream()).use { aar ->
            ZipOutputStream(target.outputStream()).use { output ->
                while (true) {
                    val entry = aar.nextEntry ?: break
                    output.putNextEntry(ZipEntry(entry.name))
                    if (entry.name == "classes.jar") {
                        val bytes = ByteArrayOutputStream()
                        ZipInputStream(ByteArrayInputStream(aar.readBytes())).use { jar ->
                            ZipOutputStream(bytes).use { filtered ->
                                while (true) {
                                    val item = jar.nextEntry ?: break
                                    if (!item.name.startsWith("org/webrtc/audio/WebRtcAudioTrack")) {
                                        filtered.putNextEntry(ZipEntry(item.name))
                                        jar.copyTo(filtered)
                                        filtered.closeEntry()
                                    }
                                }
                            }
                        }
                        output.write(bytes.toByteArray())
                    } else aar.copyTo(output)
                    output.closeEntry()
                }
            }
        }
    }
}

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.compose.material)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material3.adaptive.navigation.suite)
    implementation(libs.androidx.compose.material.icons.extended)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.okhttp)
    implementation(libs.protobuf.javalite)
    implementation(files(mediaWebRtcAar).builtBy(prepareMediaWebRtc))
    testImplementation(libs.junit)
    androidTestImplementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.espresso.core)
    androidTestImplementation(libs.androidx.junit)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    debugImplementation(libs.androidx.compose.ui.tooling)
}
