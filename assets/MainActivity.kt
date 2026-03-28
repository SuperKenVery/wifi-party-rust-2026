package dev.dioxus.main;

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import androidx.core.view.WindowCompat
import com.ken.WifiPartyRust.BuildConfig;
typealias BuildConfig = BuildConfig;

class MainActivity : WryActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        window.navigationBarColor = Color.TRANSPARENT
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        handleNativeActivityResult(requestCode, resultCode, data)
    }

    private external fun handleNativeActivityResult(requestCode: Int, resultCode: Int, data: Intent?)
}
