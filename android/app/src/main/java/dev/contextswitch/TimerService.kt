package dev.contextswitch

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * Foreground service keeping a persistent, podcast-style notification while
 * a timer runs. [NotificationCompat.Builder.setUsesChronometer] gives a live
 * ticking elapsed-time display for free — no per-second updates needed.
 */
class TimerService : Service() {

    companion object {
        const val CHANNEL_ID = "timer"
        const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "dev.contextswitch.action.STOP"
        const val ACTION_UPDATE = "dev.contextswitch.action.UPDATE"
        const val EXTRA_STARTED_AT_MS = "started_at_ms"
        const val EXTRA_LABEL = "label"

        fun start(context: Context, startedAtMs: Long, label: String) {
            context.startForegroundService(
                Intent(context, TimerService::class.java)
                    .setAction(ACTION_UPDATE)
                    .putExtra(EXTRA_STARTED_AT_MS, startedAtMs)
                    .putExtra(EXTRA_LABEL, label),
            )
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, TimerService::class.java))
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                val logbook = (application as CoswApp).logbook
                if (logbook.hasActiveTimer) logbook.stop()
                stopSelf()
            }
            else -> {
                val startedAt = intent?.getLongExtra(EXTRA_STARTED_AT_MS, 0L) ?: 0L
                val label = intent?.getStringExtra(EXTRA_LABEL).orEmpty()
                startForeground(NOTIFICATION_ID, buildNotification(startedAt, label))
            }
        }
        return START_STICKY
    }

    private fun buildNotification(startedAtMs: Long, label: String): Notification {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, getString(R.string.timer_channel_name), NotificationManager.IMPORTANCE_LOW),
        )

        val openIntent = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopIntent = PendingIntent.getService(
            this, 1,
            Intent(this, TimerService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_timer)
            .setContentTitle(label.ifBlank { getString(R.string.app_name) })
            .setOngoing(true)
            .setWhen(startedAtMs)
            .setUsesChronometer(true)
            .setContentIntent(openIntent)
            .addAction(0, "Stop", stopIntent)
            .build()
    }
}
