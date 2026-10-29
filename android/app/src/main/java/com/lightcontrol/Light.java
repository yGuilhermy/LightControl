package com.lightcontrol;

import java.util.Locale;
import org.json.JSONObject;

public final class Light {
  boolean power = false;
  String mode = "white";
  int brightness = 70, temperature = 40, hue = 35, saturation = 65, value = 70;

  Light copy() {
    Light l = new Light();
    l.power = power;
    l.mode = mode;
    l.brightness = brightness;
    l.temperature = temperature;
    l.hue = hue;
    l.saturation = saturation;
    l.value = value;
    return l;
  }

  int level() {
    return mode.equals("colour") ? value : brightness;
  }

  void level(int n) {
    if (mode.equals("colour")) value = clamp(n, 0, 100);
    else brightness = clamp(n, 1, 100);
  }

  static int clamp(int n, int a, int b) {
    return Math.max(a, Math.min(n, b));
  }

  String hsv() {
    return String.format(
        Locale.ROOT,
        "%04x%04x%04x",
        clamp(hue, 0, 360),
        clamp(saturation, 0, 100) * 10,
        clamp(value, 0, 100) * 10);
  }

  void merge(JSONObject d) {
    power = d.optBoolean("20", power);
    mode = d.optString("21", mode);
    if (d.has("22")) brightness = (int) Math.round(clamp(d.optInt("22"), 10, 1000) / 10.0);
    if (d.has("23")) temperature = clamp(d.optInt("23"), 0, 1000) / 10;
    String s = d.optString("24", "");
    if (s.matches("[0-9a-fA-F]{12}")) {
      hue = clamp(Integer.parseInt(s.substring(0, 4), 16), 0, 360);
      saturation = clamp(Integer.parseInt(s.substring(4, 8), 16) / 10, 0, 100);
      value = clamp(Integer.parseInt(s.substring(8), 16) / 10, 0, 100);
    }
  }

  JSONObject dps() {
    try {
      JSONObject d = new JSONObject().put("20", power).put("21", mode);
      if (mode.equals("colour")) d.put("24", hsv());
      else d.put("22", clamp(brightness, 1, 100) * 10).put("23", clamp(temperature, 0, 100) * 10);
      return d;
    } catch (Exception e) {
      throw new IllegalStateException(e);
    }
  }

  JSONObject json() {
    try {
      return new JSONObject()
          .put("power", power)
          .put("mode", mode)
          .put("brightness", brightness)
          .put("temperature", temperature)
          .put("hue", hue)
          .put("saturation", saturation)
          .put("value", value);
    } catch (Exception e) {
      throw new IllegalStateException(e);
    }
  }

  static Light from(JSONObject j) {
    Light l = new Light();
    if (j != null) {
      l.power = j.optBoolean("power", true);
      l.mode = j.optString("mode", "white");
      l.brightness = clamp(j.optInt("brightness", 70), 1, 100);
      l.temperature = clamp(j.optInt("temperature", 40), 0, 100);
      l.hue = clamp(j.optInt("hue", 35), 0, 360);
      l.saturation = clamp(j.optInt("saturation", 65), 0, 100);
      l.value = clamp(j.optInt("value", 70), 0, 100);
    }
    return l;
  }

  void hex(String hex) {
    int rgb = Integer.parseInt(hex.replace("#", ""), 16);
    float r = ((rgb >> 16) & 255) / 255f,
        g = ((rgb >> 8) & 255) / 255f,
        b = (rgb & 255) / 255f,
        max = Math.max(r, Math.max(g, b)),
        min = Math.min(r, Math.min(g, b)),
        delta = max - min;
    float h =
        delta == 0
            ? 0
            : max == r
                ? 60 * ((g - b) / delta % 6)
                : max == g ? 60 * ((b - r) / delta + 2) : 60 * ((r - g) / delta + 4);
    hue = Math.round((h + 360) % 360);
    saturation = max == 0 ? 0 : Math.round(delta / max * 100);
    value = Math.round(max * 100);
    mode = "colour";
  }

  int rgb() {
    float h = hue / 60f,
        s = saturation / 100f,
        v = value / 100f,
        c = v * s,
        x = c * (1 - Math.abs(h % 2 - 1)),
        m = v - c;
    float r = 0, g = 0, b = 0;
    switch ((int) h % 6) {
      case 0:
        r = c;
        g = x;
        break;
      case 1:
        r = x;
        g = c;
        break;
      case 2:
        g = c;
        b = x;
        break;
      case 3:
        g = x;
        b = c;
        break;
      case 4:
        r = x;
        b = c;
        break;
      default:
        r = c;
        b = x;
    }
    return 0xff000000
        | (Math.round((r + m) * 255) << 16)
        | (Math.round((g + m) * 255) << 8)
        | Math.round((b + m) * 255);
  }

  String hex() {
    return String.format(Locale.ROOT, "#%06X", rgb() & 0xffffff);
  }
}
