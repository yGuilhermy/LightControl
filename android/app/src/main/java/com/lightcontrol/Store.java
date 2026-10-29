package com.lightcontrol;

import android.content.Context;
import android.security.keystore.*;
import android.util.AtomicFile;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.security.*;
import java.util.*;
import javax.crypto.*;
import javax.crypto.spec.*;
import org.json.*;

final class Store {
  private final Context context;
  JSONObject config;

  Store(Context c) {
    context = c.getApplicationContext();
    try {
      config =
          new JSONObject(new String(read("settings.json", 128 * 1024), StandardCharsets.UTF_8));
    } catch (Exception e) {
      config = defaults();
    }
  }

  static JSONObject defaults() {
    try {
      JSONObject j =
          new JSONObject().put("name", "Quarto").put("last_ip", "").put("transition_ms", 0);
      JSONArray p = new JSONArray();
      String[] names = {"Leitura", "Descanso", "Concentrar", "Cinema"};
      int[] levels = {85, 25, 100, 20}, temps = {65, 0, 100, 40};
      for (int i = 0; i < 4; i++) {
        Light l = new Light();
        l.power = true;
        l.brightness = levels[i];
        l.temperature = temps[i];
        if (i == 3) {
          l.mode = "colour";
          l.hue = 265;
          l.saturation = 75;
          l.value = 20;
        }
        p.put(new JSONObject().put("name", names[i]).put("light", l.json()));
      }
      j.put("presets", p)
          .put("favorites", new JSONArray("[\"#FFB86B\",\"#FF6B82\",\"#B497FF\",\"#75D5CF\"]"))
          .put(
              "scenes",
              new JSONArray(
                  "[{\"name\":\"Aurora\",\"effect\":2,\"speed\":40,\"brightness\":50,\"colors\":[\"#FFB86B\",\"#FF6B82\",\"#B497FF\"]},{\"name\":\"Oceano\",\"effect\":2,\"speed\":25,\"brightness\":45,\"colors\":[\"#75D5CF\",\"#4679C9\"]},{\"name\":\"Festa\",\"effect\":1,\"speed\":85,\"brightness\":75,\"colors\":[\"#FF4050\",\"#4679FF\",\"#66FF88\"]}]"))
          .put("routines", new JSONArray());
      return j;
    } catch (Exception e) {
      throw new IllegalStateException(e);
    }
  }

  synchronized void save() throws Exception {
    write("settings.json", config.toString().getBytes(StandardCharsets.UTF_8));
  }

  byte[] read(String name, int max) throws IOException {
    AtomicFile f = new AtomicFile(new File(context.getFilesDir(), name));
    try (InputStream in = f.openRead();
        ByteArrayOutputStream out = new ByteArrayOutputStream()) {
      byte[] b = new byte[4096];
      int n;
      while ((n = in.read(b)) != -1) {
        if (out.size() + n > max) throw new IOException("Arquivo excede o limite");
        out.write(b, 0, n);
      }
      return out.toByteArray();
    }
  }

  private void write(String name, byte[] b) throws Exception {
    AtomicFile f = new AtomicFile(new File(context.getFilesDir(), name));
    FileOutputStream out = null;
    try {
      out = f.startWrite();
      out.write(b);
      f.finishWrite(out);
    } catch (Exception e) {
      if (out != null) f.failWrite(out);
      throw e;
    }
  }

  private SecretKey vaultKey() throws Exception {
    KeyStore s = KeyStore.getInstance("AndroidKeyStore");
    s.load(null);
    String alias = "lightcontrol.credentials.v1";
    if (s.containsAlias(alias)) return (SecretKey) s.getKey(alias, null);
    KeyGenerator g = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
    g.init(
        new KeyGenParameterSpec.Builder(
                alias, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .build());
    return g.generateKey();
  }

  synchronized JSONObject credentials() throws Exception {
    if (!new File(context.getFilesDir(), "device.vault").exists()) return null;
    byte[] b = read("device.vault", 16384);
    if (b.length < 28) throw new IOException("Credencial inválida");
    Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
    c.init(Cipher.DECRYPT_MODE, vaultKey(), new GCMParameterSpec(128, Arrays.copyOf(b, 12)));
    byte[] raw = c.doFinal(Arrays.copyOfRange(b, 12, b.length));
    try {
      return new JSONObject(new String(raw, StandardCharsets.UTF_8));
    } finally {
      Arrays.fill(raw, (byte) 0);
    }
  }

  synchronized void credentials(JSONObject j) throws Exception {
    Tuya.validateIp(j.getString("ip"));
    if (j.getString("id").trim().isEmpty()
        || j.getString("key").getBytes(StandardCharsets.UTF_8).length != 16)
      throw new IOException("ID obrigatório e local_key com 16 bytes");
    Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
    c.init(Cipher.ENCRYPT_MODE, vaultKey());
    byte[] raw = j.toString().getBytes(StandardCharsets.UTF_8);
    try {
      byte[] cipher = c.doFinal(raw),
          iv = c.getIV(),
          all = Arrays.copyOf(iv, iv.length + cipher.length);
      System.arraycopy(cipher, 0, all, iv.length, cipher.length);
      write("device.vault", all);
      config.put("last_ip", j.getString("ip"));
      save();
    } finally {
      Arrays.fill(raw, (byte) 0);
    }
  }

  void importEncrypted(byte[] bytes, char[] password) throws Exception {
    if (bytes.length > 16384) throw new IOException("Arquivo de transferência inválido");
    JSONObject j = new JSONObject(new String(bytes, StandardCharsets.UTF_8));
    if (j.getInt("version") != 1) throw new IOException("Versão incompatível");
    int rounds = j.getInt("iterations");
    if (rounds < 210000 || rounds > 1000000) throw new IOException("Parâmetros inválidos");
    byte[] salt = Base64.getDecoder().decode(j.getString("salt")),
        iv = Base64.getDecoder().decode(j.getString("iv")),
        cipher = Base64.getDecoder().decode(j.getString("ciphertext"));
    if (salt.length != 16 || iv.length != 12) throw new IOException("Arquivo inválido");
    PBEKeySpec spec = new PBEKeySpec(password, salt, rounds, 256);
    byte[] k = null, raw = null;
    try {
      k = SecretKeyFactory.getInstance("PBKDF2WithHmacSHA256").generateSecret(spec).getEncoded();
      raw =
          Tuya.aes(
              k,
              iv,
              "LightControl transfer v1".getBytes(StandardCharsets.UTF_8),
              cipher,
              Cipher.DECRYPT_MODE);
      credentials(new JSONObject(new String(raw, StandardCharsets.UTF_8)));
    } finally {
      spec.clearPassword();
      Arrays.fill(password, '\0');
      if (k != null) Arrays.fill(k, (byte) 0);
      if (raw != null) Arrays.fill(raw, (byte) 0);
    }
  }

  JSONArray array(String name) {
    JSONArray a = config.optJSONArray(name);
    return a == null ? new JSONArray() : a;
  }
}
