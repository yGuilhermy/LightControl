package com.lightcontrol;

import java.util.Locale;
import org.json.*;

final class Scene {
  static String payload(JSONObject s) throws Exception {
    JSONArray colors = s.getJSONArray("colors");
    int effect = s.getInt("effect"), speed = s.getInt("speed"), brightness = s.getInt("brightness");
    if (effect < 0
        || effect > 2
        || speed < 1
        || speed > 100
        || brightness < 1
        || brightness > 100
        || colors.length() < 1
        || colors.length() > 8)
      throw new Exception("Confira a cena: 1–8 cores e valores de 1 a 100");
    StringBuilder p = new StringBuilder("00");
    for (int i = 0; i < colors.length(); i++) {
      String hex = colors.getString(i);
      if (!hex.matches("#[0-9a-fA-F]{6}")) throw new Exception("Cor inválida");
      Light l = new Light();
      l.hex(hex);
      l.value = brightness;
      p.append(
          String.format(
              Locale.ROOT, "%02x%02x%02x%s00000000", 101 - speed, 101 - speed, effect, l.hsv()));
    }
    return p.toString();
  }
}
