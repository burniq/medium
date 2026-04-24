plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.homeworks.overlay"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.homeworks.overlay"
        minSdk = 28
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.15"
    }
}

dependencies {
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.material3:material3:1.3.1")
    testImplementation("junit:junit:4.13.2")
}
