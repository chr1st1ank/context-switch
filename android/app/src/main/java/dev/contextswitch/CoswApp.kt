package dev.contextswitch

import android.app.Application
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ProcessLifecycleOwner

class CoswApp : Application() {
    lateinit var logbook: LogbookStore
        private set

    override fun onCreate() {
        super.onCreate()
        logbook = LogbookStore(this)
        logbook.open()

        ProcessLifecycleOwner.get().lifecycle.addObserver(
            LifecycleEventObserver { _, event ->
                when (event) {
                    Lifecycle.Event.ON_START -> logbook.onAppForeground()
                    Lifecycle.Event.ON_STOP -> logbook.onAppBackground()
                    else -> {}
                }
            },
        )
    }
}
