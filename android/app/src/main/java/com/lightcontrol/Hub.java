package com.lightcontrol;

import android.content.*;
import android.net.*;
import android.os.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.concurrent.*;
import java.util.function.Consumer;
import org.json.*;

final class Hub {
  interface Listener {
    void changed();
  }

  private static volatile Hub instance;

  static Hub get(Context c) {
    if (instance == null)
      synchronized (Hub.class) {
        if (instance == null) instance = new Hub(c);
      }
    return instance;
  }

  final Store store;
  final Context context;
  final Handler main = new Handler(Looper.getMainLooper());
  final ScheduledThreadPoolExecutor io =
      new ScheduledThreadPoolExecutor(
          1,
          r -> {
            Thread t = new Thread(r, "LightControl LAN");
            t.setPriority(Thread.NORM_PRIORITY - 1);
            return t;
          });
  private final Set<Listener> listeners = new HashSet<>();
  private Tuya client;
  private long connectedAt, lastHeartbeat, lastQuery, backoff = 2000, nextConnect = 0;
  volatile Light light = new Light(), staged, requested;
  volatile boolean online = false, known = false, pending = false;
  volatile String message = "Conectando à lâmpada…";
  volatile long countdownDeadline = 0;
  volatile int countdown = 0;
  volatile boolean timerCapable = false, dndCapable = false, dnd = false;
  volatile long latencyMs = 0;
  private ScheduledFuture<?> ticker, slider;
  private JSONObject queuedSlider;
  private long sliderIssued;
  private Runnable idleClose;
  private int leases;
  private volatile long generation;
  private volatile long editRevision;
  private long sliderRevision;
  private long fadeRevision;

  private Hub(Context c) {
    context = c.getApplicationContext();
    store = new Store(context);
    io.setRemoveOnCancelPolicy(true);
    ConnectivityManager cm =
        (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
    cm.registerNetworkCallback(
        new NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(),
        new ConnectivityManager.NetworkCallback() {
          @Override
          public void onAvailable(Network n) {
            io.execute(
                () -> {
                  disconnect();
                  nextConnect = 0;
                  if (active()) tick();
                });
          }

          @Override
          public void onLost(Network n) {
            io.execute(
                () -> {
                  disconnect();
                  message = "Wi-Fi desconectado";
                  notifyState();
                });
          }
        });
  }

  synchronized boolean active() {
    return leases > 0;
  }

  synchronized void acquire() {
    leases++;
    if (idleClose != null) main.removeCallbacks(idleClose);
    if (ticker == null || ticker.isCancelled())
      ticker = io.scheduleWithFixedDelay(this::tick, 0, 180, TimeUnit.MILLISECONDS);
  }

  synchronized void release() {
    leases = Math.max(0, leases - 1);
    if (leases == 0) {
      idleClose =
          () ->
              io.execute(
                  () -> {
                    synchronized (Hub.this) {
                      if (leases == 0) {
                        if (ticker != null) ticker.cancel(false);
                        disconnect();
                      }
                    }
                  });
      main.postDelayed(idleClose, 2500);
    }
  }

  void listen(Listener l) {
    synchronized (listeners) {
      listeners.add(l);
    }
    acquire();
    main.post(l::changed);
  }

  void unlisten(Listener l) {
    synchronized (listeners) {
      listeners.remove(l);
    }
    release();
  }

  private void notifyState() {
    main.post(
        () -> {
          Listener[] ls;
          synchronized (listeners) {
            ls = listeners.toArray(new Listener[0]);
          }
          for (Listener l : ls) l.changed();
        });
  }

  private void disconnect() {
    if (client != null) client.close();
    client = null;
    online = false;
    pending = false;
    generation++;
    fadeRevision++;
    editRevision++;
    requested = null;
    synchronized (this) {
      queuedSlider = null;
      if (slider != null) slider.cancel(false);
    }
  }

  private boolean connect() throws Exception {
    if (client != null && online) return true;
    message = "Conectando à lâmpada…";
    notifyState();
    JSONObject cred = store.credentials();
    if (cred == null) throw new Exception("Configure a chave local em Ajustes");
    byte[] k = cred.getString("key").getBytes(StandardCharsets.UTF_8);
    try {
      client = new Tuya(cred.getString("ip"), k);
      apply(client.query());
      connectedAt = SystemClock.elapsedRealtime();
      lastHeartbeat = lastQuery = connectedAt;
      backoff = 2000;
      return true;
    } finally {
      Arrays.fill(k, (byte) 0);
    }
  }

  private void failed(Exception e) {
    disconnect();
    message =
        e instanceof SecurityException
            ? "Permita acesso à rede local em Ajustes"
            : e instanceof javax.crypto.AEADBadTagException
                ? "Chave local inválida ou mensagem rejeitada"
                : e.getMessage() == null ? "Falha na conexão local" : e.getMessage();
    if (e instanceof java.net.ConnectException) message = "Lâmpada indisponível na rede local";
    else if (e instanceof java.net.SocketTimeoutException)
      message = "A lâmpada não confirmou a operação a tempo";
    else if (e instanceof java.net.SocketException) message = "Conexão local interrompida";
    nextConnect = SystemClock.elapsedRealtime() + backoff;
    backoff = Math.min(30000, backoff * 2);
    notifyState();
  }

  private void tick() {
    if (!active()) return;
    try {
      long now = SystemClock.elapsedRealtime();
      if (client == null) {
        if (now >= nextConnect) connect();
        return;
      }
      JSONObject d = client.poll();
      if (d != null) apply(d);
      if (now - lastQuery >= 30000) {
        apply(client.query());
        lastQuery = now;
      } else if (now - lastHeartbeat >= 10000) {
        client.heartbeat();
        lastHeartbeat = now;
      }
    } catch (Exception e) {
      failed(e);
    }
  }

  private void apply(JSONObject d) {
    Light l = light.copy();
    l.merge(d);
    light = l;
    known = true;
    online = true;
    pending = false;
    if (l.power) staged = null;
    if (d.has("26")) {
      timerCapable = true;
      int n = d.optInt("26");
      if (n == 0) countdownDeadline = 0;
      else if (n != countdown || countdownDeadline == 0)
        countdownDeadline = System.currentTimeMillis() + n * 1000L;
      countdown = n;
    }
    if (d.has("34")) {
      dndCapable = true;
      this.dnd = d.optBoolean("34");
    }
    message = "Conectada pela rede local";
    notifyState();
  }

  Light display() {
    Light s = requested != null ? requested : staged;
    return s != null ? s.copy() : light.copy();
  }

  interface Write {
    JSONObject create() throws Exception;
  }

  void command(Write write, Consumer<Boolean> result) {
    long issued = SystemClock.elapsedRealtime(), epoch = generation;
    acquire();
    io.execute(
        () -> {
          boolean ok = false;
          try {
            if (issued + 6000 < SystemClock.elapsedRealtime() || epoch != generation) {
              message = "Comando descartado: conexão mudou ou prazo expirou";
              notifyState();
              return;
            }
            try {
              connect();
            } catch (Exception e) {
              failed(e);
              return;
            }
            if (issued + 6000 < SystemClock.elapsedRealtime()) {
              message = "Comando expirou; tente novamente";
              notifyState();
              return;
            }
            JSONObject d;
            try {
              d = write.create();
            } catch (Exception e) {
              message = e.getMessage();
              notifyState();
              return;
            }
            try {
              pending = true;
              notifyState();
              long start = SystemClock.elapsedRealtime();
              apply(client.set(d));
              latencyMs = SystemClock.elapsedRealtime() - start;
              ok = true;
            } catch (Exception e) {
              failed(e);
            }
          } finally {
            boolean success = ok;
            if (result != null) main.post(() -> result.accept(success));
            release();
          }
        });
  }

  synchronized void cancelSlider() {
    queuedSlider = null;
    requested = null;
    editRevision++;
    fadeRevision++;
    if (slider != null) slider.cancel(false);
    slider = null;
  }

  void toggle() {
    cancelSlider();
    command(
        () -> {
          boolean on = !light.power;
          JSONObject d = on && staged != null ? staged.dps() : new JSONObject();
          d.put("20", on);
          if (!on && timerCapable && countdown > 0) d.put("26", 0);
          return d;
        },
        null);
  }

  void scene(JSONObject scene) {
    cancelSlider();
    command(
        () -> new JSONObject().put("20", true).put("21", "scene").put("25", Scene.payload(scene)),
        null);
  }

  void power(boolean on, Consumer<Boolean> result) {
    cancelSlider();
    command(
        () -> {
          JSONObject d = on && staged != null ? staged.dps() : new JSONObject();
          d.put("20", on);
          if (!on && timerCapable && countdown > 0) d.put("26", 0);
          return d;
        },
        result);
  }

  void setLight(Light target) {
    if (!light.power) {
      Light s = target.copy();
      s.power = false;
      staged = s;
      notifyState();
      return;
    }
    requested = target.copy();
    sendSlider(target.dps(), false);
  }

  synchronized void sendSlider(JSONObject d, boolean immediate) {
    fadeRevision++;
    queuedSlider = d;
    sliderIssued = SystemClock.elapsedRealtime();
    sliderRevision = ++editRevision;
    if (slider != null && !slider.isDone()) {
      if (!immediate) return;
      slider.cancel(false);
    }
    slider =
        io.schedule(
            () -> {
              JSONObject next;
              long issued, revision, epoch = generation;
              synchronized (Hub.this) {
                next = queuedSlider;
                issued = sliderIssued;
                revision = sliderRevision;
                queuedSlider = null;
                slider = null;
              }
              if (next != null && SystemClock.elapsedRealtime() - issued < 1500) {
                if (online)
                  command(
                      () -> {
                        if (epoch != generation || revision != editRevision)
                          throw new Exception("Ajuste substituído");
                        return next;
                      },
                      ok -> {
                        if (revision == editRevision) {
                          requested = null;
                          notifyState();
                        }
                      });
                else {
                  requested = null;
                  message = "Comando não enviado: lâmpada offline";
                  notifyState();
                }
              }
            },
            immediate ? 0 : 110,
            TimeUnit.MILLISECONDS);
  }

  void finalSlider(Light target) {
    if (!light.power) {
      setLight(target);
      return;
    }
    requested = target.copy();
    sendSlider(target.dps(), true);
  }

  void preset(Light l) {
    cancelSlider();
    Light target = l.copy();
    target.power = true;
    long duration = Light.clamp(store.config.optInt("transition_ms", 0), 0, 60000);
    if (duration == 0 || !online || !light.power) {
      command(target::dps, null);
      return;
    }
    long revision = ++fadeRevision;
    Light start = light.copy();
    long began = SystemClock.elapsedRealtime();
    fade(start, target, began, duration, revision);
  }

  private void fade(Light start, Light target, long began, long duration, long revision) {
    if (revision != fadeRevision) return;
    float f = Math.min(1, (SystemClock.elapsedRealtime() - began) / (float) duration);
    Light next = target.copy();
    next.brightness = Math.round(start.brightness + (target.brightness - start.brightness) * f);
    next.temperature = Math.round(start.temperature + (target.temperature - start.temperature) * f);
    next.hue = Math.round(start.hue + (target.hue - start.hue) * f);
    next.saturation = Math.round(start.saturation + (target.saturation - start.saturation) * f);
    next.value = Math.round(start.value + (target.value - start.value) * f);
    command(
        () -> {
          if (revision != fadeRevision) throw new Exception("Transição substituída");
          return next.dps();
        },
        ok -> {
          if (ok && f < 1 && revision == fadeRevision)
            main.postDelayed(() -> fade(start, target, began, duration, revision), 200);
        });
  }

  void timer(int minutes) {
    command(
        () -> {
          if (!timerCapable) throw new Exception("Contador da lâmpada não confirmado");
          if (!light.power && minutes > 0)
            throw new Exception("Ligue a lâmpada para usar seu contador");
          return new JSONObject().put("26", minutes * 60);
        },
        null);
  }

  void refresh() {
    io.execute(
        () -> {
          try {
            connect();
            apply(client.query());
          } catch (Exception e) {
            failed(e);
          }
        });
  }

  void reconnect() {
    io.execute(
        () -> {
          disconnect();
          nextConnect = 0;
          tick();
        });
  }

  void discover() {
    acquire();
    io.execute(
        () -> {
          try {
            message = "Procurando na rede local…";
            notifyState();
            JSONObject c = store.credentials();
            if (c == null) throw new Exception("Configure o ID e a chave primeiro");
            String ip = Tuya.discover(c.getString("id"));
            byte[] key = c.getString("key").getBytes(StandardCharsets.UTF_8);
            try (Tuya probe = new Tuya(ip, key)) {
              probe.query();
            } finally {
              Arrays.fill(key, (byte) 0);
            }
            c.put("ip", ip);
            store.credentials(c);
            disconnect();
            nextConnect = 0;
            connect();
          } catch (Exception e) {
            failed(e);
          } finally {
            release();
          }
        });
  }
}
