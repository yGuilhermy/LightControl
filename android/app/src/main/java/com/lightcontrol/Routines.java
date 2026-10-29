package com.lightcontrol;

import android.app.*;
import android.content.*;
import android.os.*;
import java.util.*;
import org.json.*;

public final class Routines extends BroadcastReceiver {
  static PendingIntent intent(Context c, int id, long due) {
    return PendingIntent.getBroadcast(
        c,
        id,
        new Intent(c, Routines.class)
            .setAction("com.lightcontrol.ROUTINE")
            .putExtra("index", id)
            .putExtra("due", due),
        PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
  }

  static void schedule(Context c) {
    Store store = Hub.get(c).store;
    AlarmManager am = (AlarmManager) c.getSystemService(Context.ALARM_SERVICE);
    for (int i = 0; i < 128; i++) am.cancel(intent(c, i, 0));
    JSONArray routines = store.array("routines");
    long now = System.currentTimeMillis();
    for (int i = 0; i < Math.min(128, routines.length()); i++) {
      JSONObject r = routines.optJSONObject(i);
      if (r == null || !r.optBoolean("enabled", true)) continue;
      long due = next(r, now);
      if (due == 0) continue;
      PendingIntent p = intent(c, i, due);
      if (Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms())
        am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, due, p);
      else am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, due, p);
    }
  }

  static long next(JSONObject r, long now) {
    Calendar d = Calendar.getInstance();
    d.setTimeInMillis(now);
    for (int i = 0; i < 8; i++) {
      Calendar t = (Calendar) d.clone();
      t.add(Calendar.DAY_OF_YEAR, i);
      t.set(Calendar.HOUR_OF_DAY, Light.clamp(r.optInt("hour", 22), 0, 23));
      t.set(Calendar.MINUTE, Light.clamp(r.optInt("minute", 0), 0, 59));
      t.set(Calendar.SECOND, 0);
      t.set(Calendar.MILLISECOND, 0);
      if (t.getTimeInMillis() > now
          && ((r.optInt("days", 127) & (1 << (t.get(Calendar.DAY_OF_WEEK) - 1))) != 0))
        return t.getTimeInMillis();
    }
    return 0;
  }

  @Override
  public void onReceive(Context c, Intent intent) {
    Hub h = Hub.get(c);
    if (!"com.lightcontrol.ROUTINE".equals(intent.getAction())) {
      schedule(c);
      return;
    }
    int i = intent.getIntExtra("index", -1);
    long due = intent.getLongExtra("due", 0);
    JSONObject r = h.store.array("routines").optJSONObject(i);
    schedule(c);
    if (r == null
        || !r.optBoolean("enabled", true)
        || due == 0
        || Math.abs(System.currentTimeMillis() - due) > 120000) return;
    String occurrence =
        "routine."
            + i
            + "."
            + new java.text.SimpleDateFormat("yyyyMMdd", java.util.Locale.ROOT)
                .format(new java.util.Date(due));
    android.content.SharedPreferences log =
        c.getSharedPreferences("routine_runs", Context.MODE_PRIVATE);
    if (log.getBoolean(occurrence, false)) return;
    log.edit().putBoolean(occurrence, true).apply();
    PendingResult pending = goAsync();
    PowerManager.WakeLock lock =
        ((PowerManager) c.getSystemService(Context.POWER_SERVICE))
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "LightControl:routine");
    lock.acquire(15000);
    Light target = Light.from(r.optJSONObject("light"));
    h.command(
        () -> {
          org.json.JSONObject d = target.dps();
          if (!target.power && h.timerCapable && h.countdown > 0) d.put("26", 0);
          return d;
        },
        ok -> {
          log.edit()
              .putString(
                  "last_result",
                  ok ? "Rotina executada" : "Rotina não executada: sem conexão local")
              .apply();
          if (lock.isHeld()) lock.release();
          pending.finish();
        });
  }
}
