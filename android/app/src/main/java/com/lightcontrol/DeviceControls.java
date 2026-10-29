package com.lightcontrol;

import android.app.PendingIntent;
import android.content.Intent;
import android.service.controls.*;
import android.service.controls.actions.*;
import android.service.controls.templates.*;
import java.util.*;
import java.util.concurrent.Flow;
import java.util.function.Consumer;

public class DeviceControls extends ControlsProviderService {
  static final String ID = "quarto.light";
  private Hub hub;
  private final Set<ControlSubscription> subscriptions =
      java.util.concurrent.ConcurrentHashMap.newKeySet();

  @Override
  public void onDestroy() {
    for (ControlSubscription s : subscriptions.toArray(new ControlSubscription[0])) s.cancel();
    super.onDestroy();
  }

  @Override
  public void onCreate() {
    super.onCreate();
    hub = Hub.get(this);
  }

  private Control control(boolean stateless) {
    PendingIntent open =
        PendingIntent.getActivity(
            this,
            0,
            new Intent(this, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    String name = hub.store.config.optString("name", "Quarto");
    if (stateless)
      return new Control.StatelessBuilder(ID, open)
          .setTitle(name)
          .setSubtitle("LightControl · rede local")
          .setStructure("LightControl")
          .setDeviceType(DeviceTypes.TYPE_LIGHT)
          .build();
    Light l = hub.light;
    int status = hub.online && hub.known ? Control.STATUS_OK : Control.STATUS_ERROR;
    Control.StatefulBuilder b =
        new Control.StatefulBuilder(ID, open)
            .setTitle(name)
            .setSubtitle("Luz do quarto")
            .setStructure("LightControl")
            .setDeviceType(DeviceTypes.TYPE_LIGHT)
            .setStatus(status)
            .setStatusText(
                hub.online
                    ? (l.power
                        ? l.level() + "% · " + (l.mode.equals("colour") ? "Cor" : "Branco")
                        : "Desligada")
                    : hub.message)
            .setControlTemplate(
                new ToggleRangeTemplate(
                    "light",
                    l.power,
                    l.power ? "Desligar" : "Ligar",
                    new RangeTemplate("brightness", 0, 100, l.level(), 1, "%.0f%%")));
    return b.build();
  }

  @Override
  public Flow.Publisher<Control> createPublisherForAllAvailable() {
    return publisher(true, Collections.singletonList(ID));
  }

  @Override
  public Flow.Publisher<Control> createPublisherFor(List<String> ids) {
    return publisher(false, ids);
  }

  private Flow.Publisher<Control> publisher(boolean finite, List<String> ids) {
    return subscriber -> {
      if (subscriber == null) throw new NullPointerException();
      ControlSubscription subscription =
          new ControlSubscription(subscriber, finite, ids.contains(ID));
      subscriptions.add(subscription);
      subscriber.onSubscribe(subscription);
    };
  }

  private final class ControlSubscription implements Flow.Subscription, Hub.Listener {
    final Flow.Subscriber<? super Control> subscriber;
    final boolean finite, selected;
    long demand;
    boolean cancelled, listening, dirty = true;

    ControlSubscription(Flow.Subscriber<? super Control> s, boolean f, boolean selected) {
      subscriber = s;
      finite = f;
      this.selected = selected;
    }

    @Override
    public void request(long n) {
      hub.main.post(
          () -> {
            if (cancelled) return;
            if (n <= 0) {
              cancel();
              subscriber.onError(new IllegalArgumentException("Demand must be positive"));
              return;
            }
            demand = demand > Long.MAX_VALUE - n ? Long.MAX_VALUE : demand + n;
            if (!selected) {
              cancelled = true;
              subscriptions.remove(this);
              subscriber.onComplete();
              return;
            }
            if (!finite && !listening) {
              listening = true;
              hub.listen(this);
            }
            emit();
          });
    }

    @Override
    public void cancel() {
      hub.main.post(
          () -> {
            if (cancelled) return;
            cancelled = true;
            subscriptions.remove(this);
            if (listening) hub.unlisten(this);
          });
    }

    @Override
    public void changed() {
      dirty = true;
      emit();
    }

    private void emit() {
      if (cancelled || !dirty || demand == 0) return;
      dirty = false;
      demand--;
      try {
        subscriber.onNext(control(finite));
        if (finite) {
          cancelled = true;
          subscriptions.remove(this);
          subscriber.onComplete();
        }
      } catch (Throwable e) {
        cancel();
      }
    }
  }

  @Override
  public void performControlAction(String id, ControlAction action, Consumer<Integer> consumer) {
    if (!ID.equals(id)) {
      consumer.accept(ControlAction.RESPONSE_FAIL);
      return;
    }
    if (action instanceof BooleanAction && "light".equals(action.getTemplateId()))
      hub.power(
          ((BooleanAction) action).getNewState(),
          ok -> consumer.accept(ok ? ControlAction.RESPONSE_OK : ControlAction.RESPONSE_FAIL));
    else if (action instanceof FloatAction && "brightness".equals(action.getTemplateId())) {
      float value = ((FloatAction) action).getNewValue();
      if (!Float.isFinite(value) || value < 0 || value > 100) {
        consumer.accept(ControlAction.RESPONSE_FAIL);
        return;
      }
      hub.cancelSlider();
      hub.command(
          () -> {
            Light l = hub.display();
            if (l.mode.equals("scene")) l.mode = "white";
            l.level(Math.round(value));
            l.power = true;
            return l.dps();
          },
          ok -> consumer.accept(ok ? ControlAction.RESPONSE_OK : ControlAction.RESPONSE_FAIL));
    } else consumer.accept(ControlAction.RESPONSE_FAIL);
  }
}
