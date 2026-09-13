package dev.contextswitch

import android.app.Application

class CoswApp : Application() {
    lateinit var logbook: LogbookStore
        private set

    override fun onCreate() {
        super.onCreate()
        logbook = LogbookStore(this)
        logbook.open()
    }
}
