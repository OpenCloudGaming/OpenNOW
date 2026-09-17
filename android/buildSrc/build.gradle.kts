plugins { java }

repositories {
    google()
    mavenCentral()
}

dependencies {
    // Keep aligned with the Android plugin in ../build.gradle.kts.
    implementation("com.android.tools.build:gradle-api:9.4.0") {
        // The app supplies the plugins; loading them here shadows its Kotlin/AGP versions.
        exclude(group = "com.android.tools.build", module = "gradle")
        exclude(group = "org.jetbrains.kotlin", module = "kotlin-gradle-plugin-api")
    }
    implementation("org.ow2.asm:asm:9.9")
    testImplementation("junit:junit:4.13.2")
}

java { toolchain { languageVersion.set(JavaLanguageVersion.of(17)) } }
