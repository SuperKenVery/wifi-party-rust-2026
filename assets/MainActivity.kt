package dev.dioxus.main;

import android.content.Intent
import com.ken.WifiPartyRust.BuildConfig;
typealias BuildConfig = BuildConfig;

class MainActivity : WryActivity() {

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        handleNativeActivityResult(requestCode, resultCode, data)
    }

    private external fun handleNativeActivityResult(requestCode: Int, resultCode: Int, data: Intent?)
}
