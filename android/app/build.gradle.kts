plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

val buildingPlayReleaseBundle =
    providers.gradleProperty("distribution").orNull.equals("play-store", ignoreCase = true) ||
        gradle.startParameter.taskNames.any { taskName ->
            taskName.substringAfterLast(":").equals("bundleRelease", ignoreCase = true)
        }

android {
    namespace = "com.opencloudgaming.opennow"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.opencloudgaming.opennow"
        minSdk = 23
        // Android 17 target changes are audited; LAN access is permission-gated at its feature boundary.
        //noinspection EditedTargetSdkVersion
        targetSdk = 37
        versionCode = 124
        versionName = "1.6.8"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        buildConfigField("boolean", "APK_UPDATES_SUPPORTED", "true")
        buildConfigField("boolean", "PLAY_STORE_RELEASE", "false")
        buildConfigField("boolean", "LOCAL_APP_LAUNCHER_SUPPORTED", "true")

        ndk {
            // Keep legacy Intel TV devices eligible; App Bundles deliver only the matching ABI.
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
        }

    }

    buildTypes {
        debug {
            isMinifyEnabled = false
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            buildConfigField("boolean", "APK_UPDATES_SUPPORTED", (!buildingPlayReleaseBundle).toString())
            buildConfigField("boolean", "PLAY_STORE_RELEASE", buildingPlayReleaseBundle.toString())
            buildConfigField("boolean", "LOCAL_APP_LAUNCHER_SUPPORTED", (!buildingPlayReleaseBundle).toString())
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    sourceSets {
        getByName("debug").manifest.srcFile("src/sideload/AndroidManifest.xml")
        getByName("release").manifest.srcFile(
            if (buildingPlayReleaseBundle) {
                "src/playBundle/AndroidManifest.xml"
            } else {
                "src/sideload/AndroidManifest.xml"
            },
        )
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    packaging {
        jniLibs {
            useLegacyPackaging = false
        }
        resources {
            excludes += setOf(
                "META-INF/AL2.0",
                "META-INF/LGPL2.1",
                "META-INF/LICENSE*",
                "META-INF/NOTICE*",
            )
        }
    }

    lint {
        checkReleaseBuilds = false
    }
}

val nvstJniOutput = layout.buildDirectory.dir("generated/nvstJniLibs")
val buildNvst by tasks.registering(Exec::class) {
    inputs.files(fileTree(rootProject.file("nvst")) { exclude("target/**") })
    inputs.file(rootProject.file("scripts/build_nvst.py"))
    outputs.dir(nvstJniOutput)
    val ndkDirectory = androidComponents.sdkComponents.ndkDirectory
    commandLine(
        if (System.getProperty("os.name").startsWith("Windows")) "python" else "python3",
        rootProject.file("scripts/build_nvst.py").absolutePath,
        ndkDirectory.get().asFile.absolutePath,
        nvstJniOutput.get().asFile.absolutePath,
    )
}
android.sourceSets.getByName("main").jniLibs.srcDir(nvstJniOutput.get().asFile)
tasks.named("preBuild").configure { dependsOn(buildNvst) }

kotlin {
    jvmToolchain(17)
    compilerOptions {
        // K2's FIR data-flow analysis is pathological on the streaming state machine. Kotlin's
        // supported 1.9 language mode keeps the stable frontend until that class is fully decomposed.
        languageVersion.set(org.jetbrains.kotlin.gradle.dsl.KotlinVersion.KOTLIN_1_9)
        // This app is one large Kotlin module. Parallelize JVM code generation so cold builds
        // do not leave the machine idling on a single backend thread.
        freeCompilerArgs.add("-Xbackend-threads=0")
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.08.00"))
    androidTestImplementation(platform("androidx.compose:compose-bom:2026.08.00"))

    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.browser:browser:1.10.0")
    // Play App Update still requests Fragment 1.0.0 transitively through Play Services.
    implementation("androidx.fragment:fragment:1.9.0")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.core:core:1.19.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
    implementation("androidx.media3:media3-common:1.11.0")
    implementation("androidx.media3:media3-exoplayer:1.11.0")
    implementation("androidx.media3:media3-ui:1.11.0")
    implementation("androidx.work:work-runtime:2.11.2")

    implementation("io.coil-kt.coil3:coil-compose:3.5.0")
    implementation("io.coil-kt.coil3:coil-network-okhttp:3.5.0")

    implementation("com.squareup.okhttp3:logging-interceptor:5.5.0")
    implementation("com.squareup.okhttp3:okhttp-dnsoverhttps:5.5.0")
    implementation("com.squareup.okhttp3:okhttp:5.5.0")
    implementation("com.google.android.play:app-update:2.1.0")
    implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
    implementation("com.google.mlkit:language-id:17.0.6")
    // Includes upstream Android AudioRecord restart and stopped-transceiver stats crash fixes.
    implementation("io.github.webrtc-sdk:android:144.7559.14")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")

    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")

    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.7.0")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
}
