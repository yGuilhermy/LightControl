package com.lightcontrol;

import static com.lightcontrol.Widgets.*;

import android.app.*;
import android.content.*;
import android.content.res.ColorStateList;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.*;
import android.os.*;
import android.provider.Settings;
import android.text.InputType;
import android.view.*;
import android.widget.*;
import java.io.*;
import java.util.*;
import java.util.function.Consumer;
import org.json.*;

public final class MainActivity extends Activity implements Hub.Listener {
  Hub hub;
  LinearLayout root, body, nav, homeCard, palette;
  ScrollView scroll;
  TextView status, energy, power, percent, timerText;
  Widgets.Bulb bulb;
  Widgets.Slider brightness, temperature, hue, saturation;
  String page = "Luz", renderMode = "";
  boolean editing;
  Light draft;
  final Handler ui = new Handler(Looper.getMainLooper());
  final Runnable clock =
      new Runnable() {
        public void run() {
          if (timerText != null) timerText.setText(timerSummary());
          ui.postDelayed(this, 1000);
        }
      };

  @Override
  public void onCreate(Bundle state) {
    super.onCreate(state);
    getWindow().setStatusBarColor(BG);
    getWindow().setNavigationBarColor(BG);
    hub = Hub.get(this);
    if (state != null) page = state.getString("page", "Luz");
    createRoot();
    showPage(page);
    if (Build.VERSION.SDK_INT >= 37
        && getApplicationInfo().targetSdkVersion >= 37
        && checkSelfPermission("android.permission.ACCESS_LOCAL_NETWORK")
            != android.content.pm.PackageManager.PERMISSION_GRANTED)
      requestPermissions(new String[] {"android.permission.ACCESS_LOCAL_NETWORK"}, 70);
  }

  @Override
  protected void onStart() {
    super.onStart();
    hub.listen(this);
    ui.post(clock);
  }

  @Override
  protected void onStop() {
    hub.unlisten(this);
    ui.removeCallbacks(clock);
    super.onStop();
  }

  @Override
  protected void onSaveInstanceState(Bundle state) {
    super.onSaveInstanceState(state);
    state.putString("page", page);
  }

  boolean wide() {
    return getResources().getConfiguration().screenWidthDp >= 700;
  }

  int dp(float n) {
    return Math.round(n * getResources().getDisplayMetrics().density);
  }

  TextView text(String s, int size, int color) {
    TextView t = new TextView(this);
    t.setText(s);
    t.setTextSize(size);
    t.setTextColor(color);
    t.setFontFeatureSettings("tnum");
    t.setTypeface(Typeface.create("sans-serif", Typeface.NORMAL));
    return t;
  }

  LinearLayout column() {
    LinearLayout l = new LinearLayout(this);
    l.setOrientation(LinearLayout.VERTICAL);
    return l;
  }

  LinearLayout row() {
    LinearLayout l = new LinearLayout(this);
    l.setOrientation(LinearLayout.HORIZONTAL);
    l.setGravity(Gravity.CENTER_VERTICAL);
    return l;
  }

  GradientDrawable rounded(int color, int radius) {
    GradientDrawable d = new GradientDrawable();
    d.setColor(color);
    d.setCornerRadius(dp(radius));
    return d;
  }

  android.graphics.drawable.Drawable buttonBackground(boolean selected) {
    StateListDrawable d = new StateListDrawable();
    d.addState(new int[] {android.R.attr.state_pressed}, rounded(0xff46553b, 10));
    GradientDrawable normal = rounded(selected ? 0xff394830 : TILE, 10);
    if (selected) normal.setStroke(dp(1), 0xff687e50);
    d.addState(new int[] {}, normal);
    return d;
  }

  TextView button(String s, Runnable action) {
    return button(s, false, action);
  }

  TextView button(String s, boolean selected, Runnable action) {
    TextView b = text(s, 13, selected ? ACCENT : TEXT);
    b.setGravity(Gravity.CENTER);
    b.setMinHeight(dp(44));
    b.setPadding(dp(12), dp(6), dp(12), dp(6));
    b.setBackground(buttonBackground(selected));
    b.setClickable(true);
    b.setFocusable(true);
    b.setOnClickListener(v -> action.run());
    b.setAccessibilityDelegate(
        new View.AccessibilityDelegate() {
          @Override
          public void onInitializeAccessibilityNodeInfo(
              View host, android.view.accessibility.AccessibilityNodeInfo info) {
            super.onInitializeAccessibilityNodeInfo(host, info);
            info.setClassName("android.widget.Button");
          }
        });
    return b;
  }

  void gap(LinearLayout l, int height) {
    View v = new View(this);
    l.addView(v, new LinearLayout.LayoutParams(1, dp(height)));
  }

  void add(LinearLayout l, View v, int top) {
    LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(-1, -2);
    p.topMargin = dp(top);
    l.addView(v, p);
  }

  LinearLayout card() {
    LinearLayout c = column();
    c.setBackground(rounded(CARD, 22));
    c.setPadding(dp(22), dp(20), dp(22), dp(20));
    return c;
  }

  void createRoot() {
    root = column();
    root.setBackgroundColor(BG);
    setContentView(root);
    root.setOnApplyWindowInsetsListener(
        (v, insets) -> {
          android.graphics.Insets sys =
              insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
          root.setPadding(sys.left, sys.top, sys.right, sys.bottom);
          return insets;
        });
    root.requestApplyInsets();
    LinearLayout header = row();
    header.setPadding(dp(wide() ? 32 : 24), dp(18), dp(wide() ? 32 : 24), dp(6));
    TextView logo = text("lightcontrol", 24, TEXT);
    header.addView(
        logo,
        wide()
            ? new LinearLayout.LayoutParams(dp(192), dp(32))
            : new LinearLayout.LayoutParams(0, dp(38), 1));
    header.addView(
        text("Seu espaço em outra luz.", 10, MUTED),
        wide() ? new LinearLayout.LayoutParams(0, -2, 1) : new LinearLayout.LayoutParams(-2, -2));
    if (wide())
      header.addView(
          button("Recolher", () -> moveTaskToBack(true)),
          new LinearLayout.LayoutParams(dp(94), dp(30)));
    root.addView(header);
    nav = row();
    nav.setPadding(dp(wide() ? 26 : 18), dp(4), dp(18), dp(8));
    root.addView(nav);
    View line = new View(this);
    line.setBackgroundColor(TILE);
    root.addView(line, new LinearLayout.LayoutParams(-1, dp(1)));
    scroll = new ScrollView(this);
    scroll.setFillViewport(false);
    scroll.setClipToPadding(false);
    body = column();
    body.setPadding(dp(wide() ? 32 : 16), dp(wide() ? 13 : 16), dp(wide() ? 32 : 16), dp(16));
    scroll.addView(body);
    root.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));
    status = text("Conectando à lâmpada…", 11, MUTED);
    status.setPadding(dp(24), dp(10), dp(24), dp(12));
    status.setAccessibilityLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);
    status.setOnClickListener(v -> hub.refresh());
    root.addView(status);
  }

  void showPage(String selected) {
    page = selected;
    editing = false;
    nav.removeAllViews();
    for (String s : new String[] {"Luz", "Presets", "Cenas", "Rotinas", "Ajustes"}) {
      TextView b = button(s, s.equals(page), () -> showPage(s));
      b.setTextSize(11);
      b.setPadding(dp(2), 0, dp(2), 0);
      LinearLayout.LayoutParams p =
          wide()
              ? new LinearLayout.LayoutParams(dp(94), dp(36))
              : new LinearLayout.LayoutParams(0, dp(40), 1);
      p.setMargins(dp(3), 0, dp(3), 0);
      nav.addView(b, p);
    }
    body.removeAllViews();
    brightness = temperature = hue = saturation = null;
    timerText = null;
    bulb = null;
    power = energy = percent = null;
    switch (page) {
      case "Luz":
        home();
        break;
      case "Presets":
        presets();
        break;
      case "Cenas":
        scenes();
        break;
      case "Rotinas":
        routines();
        break;
      default:
        settings();
    }
    changed();
    scroll.scrollTo(0, 0);
  }

  void home() {
    draft = hub.display();
    renderMode = draft.mode;
    boolean wide = getResources().getConfiguration().screenWidthDp >= 700;
    LinearLayout area = wide ? row() : column();
    area.setGravity(Gravity.TOP);
    body.addView(area, new LinearLayout.LayoutParams(-1, -2));
    homeCard = card();
    if (wide()) homeCard.setPadding(dp(20), dp(16), dp(20), dp(16));
    LinearLayout.LayoutParams cp = new LinearLayout.LayoutParams(wide ? dp(362) : -1, -2);
    if (wide) cp.rightMargin = dp(20);
    area.addView(homeCard, cp);
    LinearLayout title = row(), names = column();
    names.addView(text(hub.store.config.optString("name", "Quarto"), 14, TEXT));
    energy = text("Estado desconhecido", 11, MUTED);
    add(names, energy, 6);
    title.addView(names, new LinearLayout.LayoutParams(0, -2, 1));
    power = button("Ligar", () -> hub.toggle());
    title.addView(power, new LinearLayout.LayoutParams(dp(wide() ? 80 : 88), dp(wide() ? 32 : 44)));
    homeCard.addView(title);
    bulb = new Widgets.Bulb(this);
    homeCard.addView(bulb, new LinearLayout.LayoutParams(-1, dp(wide() ? 100 : 116)));
    LinearLayout level = row();
    percent = text(draft.level() + "", 56, TEXT);
    level.addView(percent);
    TextView unit = text("%", 16, MUTED);
    LinearLayout.LayoutParams up = new LinearLayout.LayoutParams(-2, -2);
    up.leftMargin = dp(12);
    level.addView(unit, up);
    TextView label = text("Brilho", 12, MUTED);
    label.setGravity(Gravity.RIGHT | Gravity.BOTTOM);
    level.addView(label, new LinearLayout.LayoutParams(0, dp(56), 1));
    homeCard.addView(level);
    brightness =
        new Widgets.Slider(
            this,
            "Brilho",
            draft.mode.equals("colour") ? 0 : 1,
            100,
            draft.level(),
            new int[] {0xff5c6e4f, 0xffcbdcac},
            (v, end) -> {
              draft.level(v);
              edit(end);
            });
    homeCard.addView(brightness, new LinearLayout.LayoutParams(-1, dp(wide() ? 40 : 48)));
    LinearLayout modes = row();
    for (String mode : new String[] {"white", "colour"}) {
      TextView b =
          button(
              mode.equals("white") ? "Branco" : "Cor",
              mode.equals(draft.mode),
              () -> {
                Light l = hub.display();
                l.mode = mode;
                hub.setLight(l);
                draft = l;
                renderMode = mode;
                showHomeWithDraft(l);
              });
      LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(0, dp(wide() ? 32 : 40), 1);
      p.setMargins(dp(2), dp(4), dp(2), dp(4));
      modes.addView(b, p);
    }
    homeCard.addView(modes);
    if (draft.mode.equals("colour")) {
      add(homeCard, text("Tonalidade", 13, MUTED), 18);
      hue =
          new Widgets.Slider(
              this,
              "Tonalidade",
              0,
              360,
              draft.hue,
              new int[] {
                Color.RED,
                Color.YELLOW,
                Color.GREEN,
                Color.CYAN,
                Color.BLUE,
                Color.MAGENTA,
                Color.RED
              },
              (v, end) -> {
                draft.hue = v;
                edit(end);
              });
      homeCard.addView(hue, new LinearLayout.LayoutParams(-1, dp(wide() ? 40 : 48)));
      add(homeCard, text("Saturação", 13, MUTED), 6);
      saturation =
          new Widgets.Slider(
              this,
              "Saturação",
              0,
              100,
              draft.saturation,
              new int[] {0xffe4e9df, Color.HSVToColor(new float[] {draft.hue, 1, 1})},
              (v, end) -> {
                draft.saturation = v;
                edit(end);
              });
      homeCard.addView(saturation, new LinearLayout.LayoutParams(-1, dp(wide() ? 40 : 48)));
      add(
          homeCard,
          button(
              "Escolher cor",
              () ->
                  pickColor(
                      draft,
                      l -> {
                        draft = l;
                        hub.setLight(l);
                        hub.finalSlider(l);
                        changed();
                      })),
          8);
    } else {
      add(homeCard, text("Temperatura do branco", 13, MUTED), 20);
      temperature =
          new Widgets.Slider(
              this,
              "Temperatura",
              0,
              100,
              draft.temperature,
              new int[] {0xffd5a05a, 0xffd5d4ac, 0xffb8dbea},
              (v, end) -> {
                draft.temperature = v;
                edit(end);
              });
      homeCard.addView(temperature, new LinearLayout.LayoutParams(-1, dp(wide() ? 40 : 48)));
      LinearLayout labels = row();
      labels.addView(text("Quente", 10, MUTED), new LinearLayout.LayoutParams(0, -2, 1));
      labels.addView(text("Frio", 10, MUTED));
      homeCard.addView(labels);
    }
    add(homeCard, text("SUA PALETA", 10, MUTED), 24);
    palette = row();
    add(homeCard, palette, wide() ? 2 : 8);
    drawPalette();
    LinearLayout side = column();
    LinearLayout.LayoutParams sp = new LinearLayout.LayoutParams(wide ? 0 : -1, -2, wide ? 1 : 0);
    if (!wide) sp.topMargin = dp(24);
    area.addView(side, sp);
    side.addView(text("Qual é o clima de agora?", 23, TEXT));
    add(side, text("UM TOQUE PARA MUDAR", 10, MUTED), 2);
    JSONArray items = hub.store.array("presets");
    LinearLayout grid = null;
    for (int i = 0; i < Math.min(4, items.length()); i++) {
      if (i % 2 == 0) {
        grid = row();
        add(side, grid, 12);
      }
      JSONObject p = items.optJSONObject(i);
      if (p == null) continue;
      TextView b =
          button(
              "●  " + p.optString("name"), () -> hub.preset(Light.from(p.optJSONObject("light"))));
      android.text.SpannableString colored = new android.text.SpannableString(b.getText());
      Light presetLight = Light.from(p.optJSONObject("light"));
      int swatch =
          presetLight.mode.equals("colour")
              ? presetLight.rgb()
              : Widgets.mix(0xffffb86b, 0xffb8dbea, presetLight.temperature / 100f);
      colored.setSpan(
          new android.text.style.ForegroundColorSpan(swatch),
          0,
          1,
          android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
      b.setText(colored);
      b.setGravity(Gravity.CENTER_VERTICAL | Gravity.LEFT);
      LinearLayout.LayoutParams bp = new LinearLayout.LayoutParams(0, dp(48), 1);
      bp.setMargins(0, 0, dp(i % 2 == 0 ? 8 : 0), 0);
      grid.addView(b, bp);
    }
    add(side, button("Editar presets", () -> showPage("Presets")), 20);
    LinearLayout timer = card();
    timer.setPadding(dp(14), dp(14), dp(14), dp(14));
    timer.addView(text("Timer para desligar", 16, TEXT));
    timerText = text(timerSummary(), 11, MUTED);
    add(timer, timerText, 8);
    add(timer, button("Configurar timer", this::timerDialog), 8);
    add(side, timer, 24);
    add(side, text("Rotinas", 18, TEXT), 18);
    add(side, text(hub.store.array("routines").length() + " rotinas neste celular", 11, MUTED), 3);
    add(side, button("Gerenciar rotinas", () -> showPage("Rotinas")), 12);
    add(side, button("Atualizar estado", () -> hub.refresh()), 10);
  }

  void showHomeWithDraft(Light l) {
    if (hub.light.power) hub.requested = l;
    else hub.staged = l;
    showPage("Luz");
  }

  void edit(boolean end) {
    editing = !end;
    percent.setText(draft.level() + "");
    if (bulb != null) {
      bulb.light = draft.copy();
      bulb.known = hub.known;
      bulb.invalidate();
    }
    if (saturation != null) {
      saturation.colors = new int[] {0xffe4e9df, Color.HSVToColor(new float[] {draft.hue, 1, 1})};
      saturation.invalidate();
    }
    if (end) hub.finalSlider(draft);
    else hub.setLight(draft);
  }

  void drawPalette() {
    palette.removeAllViews();
    JSONArray a = hub.store.array("favorites");
    for (int i = 0; i < Math.min(6, a.length()); i++) {
      String hex = a.optString(i);
      TextView b =
          button(
              "",
              () -> {
                Light l = hub.display();
                try {
                  l.hex(hex);
                  hub.setLight(l);
                  hub.finalSlider(l);
                  changed();
                } catch (Exception e) {
                  toast("Cor inválida");
                }
              });
      b.setContentDescription("Aplicar cor " + hex);
      try {
        b.setBackground(
            new InsetDrawable(
                rounded(Color.parseColor(hex), 8), 0, dp(wide() ? 4 : 10), 0, dp(wide() ? 4 : 10)));
      } catch (Exception ignored) {
      }
      LinearLayout.LayoutParams p = new LinearLayout.LayoutParams(dp(32), dp(wide() ? 32 : 44));
      p.rightMargin = dp(8);
      palette.addView(b, p);
      b.setOnLongClickListener(
          v -> {
            new AlertDialog.Builder(this)
                .setTitle("Remover da paleta?")
                .setPositiveButton(
                    "Remover",
                    (d, w) -> {
                      for (int j = 0; j < a.length(); j++)
                        if (a.optString(j).equals(hex)) {
                          a.remove(j);
                          break;
                        }
                      save();
                      drawPalette();
                    })
                .setNegativeButton("Cancelar", null)
                .show();
            return true;
          });
    }
    View spacer = new View(this);
    palette.addView(spacer, new LinearLayout.LayoutParams(0, 1, 1));
    palette.addView(
        button(
            "+",
            () ->
                pickColor(
                    hub.display(),
                    l -> {
                      if (a.length() >= 6) {
                        toast("Até seis cores na paleta");
                        return;
                      }
                      a.put(l.hex());
                      try {
                        hub.store.config.put("favorites", a);
                      } catch (Exception ignored) {
                      }
                      save();
                      drawPalette();
                    })),
        new LinearLayout.LayoutParams(dp(40), dp(wide() ? 32 : 44)));
  }

  @Override
  public void changed() {
    if (isFinishing()) return;
    status.setText(hub.message + (hub.pending ? " · aplicando…" : ""));
    if (!page.equals("Luz") || bulb == null) return;
    if (!editing) {
      Light l = hub.display();
      if (!renderMode.equals(l.mode)) {
        showPage("Luz");
        return;
      }
      draft = l;
      percent.setText(l.level() + "");
      brightness.update(l.level());
      if (temperature != null) temperature.update(l.temperature);
      if (hue != null) hue.update(l.hue);
      if (saturation != null) saturation.update(l.saturation);
    }
    energy.setText(
        !hub.known
            ? "Estado desconhecido"
            : !hub.online ? "Lâmpada offline" : hub.light.power ? "Luz acesa" : "Luz apagada");
    power.setText(hub.light.power ? "Desligar" : "Ligar");
    bulb.light = draft.copy();
    bulb.known = hub.known;
    bulb.invalidate();
    if (timerText != null) timerText.setText(timerSummary());
  }

  String timerSummary() {
    if (hub.countdownDeadline == 0) return "Nenhum timer ativo";
    long seconds = Math.max(0, (hub.countdownDeadline - System.currentTimeMillis() + 999) / 1000);
    return String.format(Locale.getDefault(), "Desliga em %02d:%02d", seconds / 60, seconds % 60);
  }

  void title(String heading, String subtitle) {
    body.addView(text(heading, 25, TEXT));
    add(body, text(subtitle, 12, MUTED), 6);
    gap(body, 20);
  }

  void presets() {
    title("Seus presets", "Um toque para mudar o clima.");
    JSONArray a = hub.store.array("presets");
    for (int i = 0; i < a.length(); i++) {
      int index = i;
      JSONObject p = a.optJSONObject(i);
      if (p == null) continue;
      LinearLayout c = card();
      Light l = Light.from(p.optJSONObject("light"));
      c.addView(text(p.optString("name"), 18, TEXT));
      add(
          c,
          text(
              (l.mode.equals("colour") ? l.hex() : "Branco · " + l.temperature + "% frio")
                  + " · "
                  + l.level()
                  + "%",
              12,
              MUTED),
          5);
      LinearLayout actions = row();
      actions.addView(
          button("Aplicar", () -> hub.preset(l)), new LinearLayout.LayoutParams(0, dp(44), 1));
      TextView edit = button("Editar", () -> presetDialog(index));
      LinearLayout.LayoutParams ep = new LinearLayout.LayoutParams(dp(88), dp(44));
      ep.leftMargin = dp(8);
      actions.addView(edit, ep);
      add(c, actions, 12);
      add(body, c, 10);
    }
    add(body, button("+  Salvar luz atual", () -> presetDialog(-1)), 16);
  }

  EditText field(LinearLayout parent, String label, String value, int type) {
    add(parent, text(label, 12, MUTED), 12);
    EditText e = new EditText(this);
    e.setText(value);
    e.setTextColor(TEXT);
    e.setTextSize(15);
    e.setSingleLine(true);
    e.setInputType(type);
    e.setBackgroundTintList(ColorStateList.valueOf(ACCENT));
    parent.addView(e, new LinearLayout.LayoutParams(-1, dp(48)));
    return e;
  }

  LinearLayout form() {
    LinearLayout f = column();
    f.setPadding(dp(22), dp(8), dp(22), dp(12));
    return f;
  }

  AlertDialog dialog(String title, LinearLayout form, String positive, Runnable save) {
    ScrollView s = new ScrollView(this);
    s.addView(form);
    AlertDialog d =
        new AlertDialog.Builder(this)
            .setTitle(title)
            .setView(s)
            .setNegativeButton("Cancelar", null)
            .setPositiveButton(positive, null)
            .create();
    d.setOnShowListener(
        v ->
            d.getButton(AlertDialog.BUTTON_POSITIVE)
                .setOnClickListener(
                    b -> {
                      try {
                        save.run();
                        d.dismiss();
                      } catch (RuntimeException e) {
                        toast(e.getMessage() == null ? "Confira os campos" : e.getMessage());
                      }
                    }));
    d.show();
    d.getWindow().setBackgroundDrawable(rounded(CARD, 18));
    d.getButton(AlertDialog.BUTTON_POSITIVE).setTextColor(ACCENT);
    d.getButton(AlertDialog.BUTTON_NEGATIVE).setTextColor(MUTED);
    return d;
  }

  void presetDialog(int index) {
    JSONArray a = hub.store.array("presets");
    JSONObject old = a.optJSONObject(index);
    Light current = old == null ? hub.display() : Light.from(old.optJSONObject("light"));
    LinearLayout f = form();
    EditText name =
        field(
            f,
            "Nome",
            old == null ? "Novo preset" : old.optString("name"),
            InputType.TYPE_CLASS_TEXT);
    final Light[] chosen = {current};
    CheckBox white = new CheckBox(this);
    EditText level = field(f, "Brilho (%)", current.level() + "", InputType.TYPE_CLASS_NUMBER),
        temp =
            field(
                f, "Branco quente/frio (%)", current.temperature + "", InputType.TYPE_CLASS_NUMBER);
    add(
        f,
        button(
            "Escolher cor",
            () ->
                pickColor(
                    chosen[0],
                    l -> {
                      chosen[0] = l;
                      white.setChecked(false);
                      level.setText(l.level() + "");
                    })),
        12);
    white.setText("Usar branco");
    white.setChecked(!current.mode.equals("colour"));
    f.addView(white);
    AlertDialog d =
        dialog(
            "Salvar preset",
            f,
            "Salvar",
            () -> {
              try {
                requireName(name);
                Light l = chosen[0].copy();
                l.mode = white.isChecked() ? "white" : "colour";
                l.level(number(level, 1, 100));
                l.temperature = number(temp, 0, 100);
                l.power = true;
                JSONObject p =
                    new JSONObject()
                        .put("name", name.getText().toString().trim())
                        .put("light", l.json());
                if (index >= 0) a.put(index, p);
                else a.put(p);
                hub.store.config.put("presets", a);
                save();
                showPage("Presets");
              } catch (Exception e) {
                throw new IllegalArgumentException(e.getMessage());
              }
            });
    if (index >= 0)
      add(
          f,
          button(
              "Excluir preset",
              () -> {
                delete("presets", index);
                d.dismiss();
              }),
          12);
  }

  void pickColor(Light initial, Consumer<Light> accept) {
    Light l = initial.copy();
    l.mode = "colour";
    LinearLayout f = form();
    TextView preview = text("Prévia da cor", 18, TEXT);
    preview.setGravity(Gravity.CENTER);
    preview.setBackground(rounded(l.rgb(), 14));
    f.addView(preview, new LinearLayout.LayoutParams(-1, dp(80)));
    TextView code = text(l.hex(), 15, MUTED);
    add(f, code, 12);
    Runnable update =
        () -> {
          preview.setBackground(rounded(l.rgb(), 14));
          code.setText(l.hex());
        };
    add(f, text("Tonalidade", 13, MUTED), 12);
    Widgets.Slider
        hs =
            new Widgets.Slider(
                this,
                "Tonalidade",
                0,
                360,
                l.hue,
                new int[] {
                  Color.RED,
                  Color.YELLOW,
                  Color.GREEN,
                  Color.CYAN,
                  Color.BLUE,
                  Color.MAGENTA,
                  Color.RED
                },
                null),
        ss =
            new Widgets.Slider(
                this, "Intensidade", 0, 100, l.saturation, new int[] {0xffeeeeee, ACCENT}, null),
        vs =
            new Widgets.Slider(
                this, "Brilho", 0, 100, l.value, new int[] {0xff111111, ACCENT}, null);
    hs.change =
        (v, end) -> {
          l.hue = v;
          ss.colors = new int[] {0xffeeeeee, Color.HSVToColor(new float[] {v, 1, 1})};
          ss.invalidate();
          update.run();
        };
    ss.change =
        (v, end) -> {
          l.saturation = v;
          update.run();
        };
    vs.change =
        (v, end) -> {
          l.value = v;
          update.run();
        };
    f.addView(hs);
    add(f, text("Intensidade", 13, MUTED), 10);
    f.addView(ss);
    add(f, text("Brilho", 13, MUTED), 10);
    f.addView(vs);
    Runnable sync =
        () -> {
          hs.update(l.hue);
          ss.update(l.saturation);
          vs.update(l.value);
          update.run();
        };
    LinearLayout favorites = row();
    for (int i = 0; i < hub.store.array("favorites").length(); i++) {
      String hex = hub.store.array("favorites").optString(i);
      TextView b =
          button(
              "●",
              () -> {
                try {
                  l.hex(hex);
                  sync.run();
                } catch (Exception ignored) {
                }
              });
      try {
        b.setTextColor(Color.parseColor(hex));
      } catch (Exception ignored) {
      }
      favorites.addView(b, new LinearLayout.LayoutParams(0, dp(44), 1));
    }
    add(f, favorites, 12);
    EditText hex = field(f, "Código hexadecimal", l.hex(), InputType.TYPE_CLASS_TEXT);
    add(
        f,
        button(
            "Usar código",
            () -> {
              try {
                if (!hex.getText().toString().matches("#?[0-9A-Fa-f]{6}")) throw new Exception();
                l.hex(hex.getText().toString());
                sync.run();
              } catch (Exception e) {
                toast("Use seis dígitos hexadecimais");
              }
            }),
        8);
    dialog("Escolher cor", f, "Usar cor", () -> accept.accept(l.copy()));
  }

  void timerDialog() {
    LinearLayout f = form();
    f.addView(
        text("Executado pela própria lâmpada. Continua com o aplicativo fechado.", 13, MUTED));
    EditText minutes = field(f, "Desligar em minutos (1–1440)", "30", InputType.TYPE_CLASS_NUMBER);
    if (!hub.timerCapable)
      add(f, text("Conecte para confirmar o suporte ao contador.", 12, MUTED), 12);
    AlertDialog d =
        dialog(
            "Timer para desligar",
            f,
            "Ativar",
            () -> {
              int n = number(minutes, 1, 1440);
              if (!hub.timerCapable)
                throw new IllegalStateException("Conecte para confirmar o contador");
              if (!hub.light.power)
                throw new IllegalStateException("Ligue a lâmpada para usar o contador");
              hub.timer(n);
            });
    if (hub.countdownDeadline > 0)
      add(
          f,
          button(
              "Cancelar timer",
              () -> {
                hub.timer(0);
                d.dismiss();
              }),
          12);
  }

  void scenes() {
    title("Cenas", "Efeitos executados pela própria lâmpada.");
    JSONArray a = hub.store.array("scenes");
    if (a.length() == 0) add(body, text("Crie uma cena com suas cores favoritas.", 14, MUTED), 10);
    for (int i = 0; i < a.length(); i++) {
      JSONObject s = a.optJSONObject(i);
      if (s == null) continue;
      int index = i;
      LinearLayout c = card();
      c.addView(text(s.optString("name"), 18, TEXT));
      add(
          c,
          text(
              new String[] {"Estática", "Alternada", "Transição"}
                      [Light.clamp(s.optInt("effect"), 0, 2)]
                  + " · "
                  + s.optInt("brightness", 50)
                  + "%",
              12,
              MUTED),
          6);
      LinearLayout actions = row();
      actions.addView(
          button("Aplicar", () -> hub.scene(s)), new LinearLayout.LayoutParams(0, dp(44), 1));
      actions.addView(
          button("Editar", () -> sceneDialog(index)),
          new LinearLayout.LayoutParams(dp(88), dp(44)));
      add(c, actions, 12);
      add(body, c, 10);
    }
    add(body, button("+  Nova cena", () -> sceneDialog(-1)), 20);
  }

  void sceneDialog(int index) {
    JSONArray a = hub.store.array("scenes");
    JSONObject old = a.optJSONObject(index);
    LinearLayout f = form();
    EditText name = field(f, "Nome", old == null ? "Nova cena" : old.optString("name"), 1);
    Spinner effect = new Spinner(this);
    effect.setAdapter(
        new ArrayAdapter<String>(
            this,
            android.R.layout.simple_spinner_dropdown_item,
            new String[] {"Estática", "Alternada", "Transição"}));
    effect.setSelection(old == null ? 2 : old.optInt("effect"));
    add(f, effect, 12);
    EditText
        speed =
            field(f, "Velocidade (1–100)", old == null ? "40" : old.optInt("speed", 40) + "", 2),
        level =
            field(f, "Brilho (1–100)", old == null ? "50" : old.optInt("brightness", 50) + "", 2);
    JSONArray colors = old == null ? new JSONArray() : old.optJSONArray("colors");
    List<String> chosen = new ArrayList<>();
    if (colors != null) for (int i = 0; i < colors.length(); i++) chosen.add(colors.optString(i));
    if (chosen.isEmpty()) chosen.add("#FFB86B");
    LinearLayout samples = row();
    add(f, samples, 16);
    showColors(samples, chosen);
    add(
        f,
        button(
            "+ Adicionar cor",
            () -> {
              if (chosen.size() >= 8) {
                toast("Até oito cores");
                return;
              }
              pickColor(
                  hub.display(),
                  l -> {
                    chosen.add(l.hex());
                    showColors(samples, chosen);
                  });
            }),
        8);
    add(
        f,
        button(
            "Remover última cor",
            () -> {
              if (chosen.size() > 1) {
                chosen.remove(chosen.size() - 1);
                showColors(samples, chosen);
              }
            }),
        8);
    AlertDialog d =
        dialog(
            "Editar cena",
            f,
            "Salvar",
            () -> {
              try {
                requireName(name);
                JSONObject s =
                    new JSONObject()
                        .put("name", name.getText().toString().trim())
                        .put("effect", effect.getSelectedItemPosition())
                        .put("speed", number(speed, 1, 100))
                        .put("brightness", number(level, 1, 100))
                        .put("colors", new JSONArray(chosen));
                Scene.payload(s);
                if (index >= 0) a.put(index, s);
                else a.put(s);
                hub.store.config.put("scenes", a);
                save();
                showPage("Cenas");
              } catch (Exception e) {
                throw new IllegalArgumentException(e.getMessage());
              }
            });
    if (index >= 0)
      add(
          f,
          button(
              "Excluir cena",
              () -> {
                delete("scenes", index);
                d.dismiss();
              }),
          14);
  }

  Light color(String hex) {
    Light l = new Light();
    try {
      l.hex(hex);
    } catch (Exception ignored) {
    }
    return l;
  }

  void showColors(LinearLayout row, List<String> colors) {
    row.removeAllViews();
    for (int i = 0; i < colors.size(); i++) {
      int pos = i;
      TextView b =
          button(
              "●",
              () ->
                  pickColor(
                      color(colors.get(pos)),
                      l -> {
                        colors.set(pos, l.hex());
                        showColors(row, colors);
                      }));
      b.setTextColor(Color.parseColor(colors.get(i)));
      row.addView(b, new LinearLayout.LayoutParams(0, dp(48), 1));
    }
  }

  void routines() {
    title("Rotinas", "Horário local · requer acesso à lâmpada pelo Wi-Fi.");
    add(
        body,
        text(
            "O Android pode adiar ações em economia de bateria. Rotinas perdidas não são"
                + " repetidas.",
            12,
            MUTED),
        0);
    AlarmManager am = (AlarmManager) getSystemService(ALARM_SERVICE);
    if (Build.VERSION.SDK_INT >= 31 && !am.canScheduleExactAlarms())
      add(
          body,
          button(
              "Permitir horários precisos",
              () ->
                  startActivity(
                      new Intent(
                          Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM,
                          android.net.Uri.parse("package:" + getPackageName())))),
          12);
    JSONArray a = hub.store.array("routines");
    for (int i = 0; i < a.length(); i++) {
      JSONObject r = a.optJSONObject(i);
      if (r == null) continue;
      int index = i;
      LinearLayout c = card();
      c.addView(text(r.optString("name"), 18, TEXT));
      add(
          c,
          text(
              String.format(Locale.getDefault(), "%02d:%02d", r.optInt("hour"), r.optInt("minute"))
                  + " · "
                  + (r.optBoolean("enabled", true) ? "Ativa" : "Pausada"),
              14,
              MUTED),
          6);
      add(c, button("Editar rotina", () -> routineDialog(index)), 12);
      add(body, c, 12);
    }
    add(body, button("+  Nova rotina", () -> routineDialog(-1)), 20);
  }

  void routineDialog(int index) {
    JSONArray a = hub.store.array("routines");
    JSONObject old = a.optJSONObject(index);
    LinearLayout f = form();
    EditText name = field(f, "Nome", old == null ? "Nova rotina" : old.optString("name"), 1),
        time =
            field(
                f,
                "Horário (HH:mm)",
                old == null
                    ? "22:00"
                    : String.format(
                        Locale.ROOT, "%02d:%02d", old.optInt("hour"), old.optInt("minute")),
                InputType.TYPE_CLASS_DATETIME | InputType.TYPE_DATETIME_VARIATION_TIME);
    CheckBox enabled = new CheckBox(this);
    enabled.setText("Rotina ativa");
    enabled.setChecked(old == null || old.optBoolean("enabled", true));
    add(f, enabled, 8);
    CheckBox[] days = new CheckBox[7];
    String[] labels = {"Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"};
    for (int i = 0; i < 7; i++) {
      days[i] = new CheckBox(this);
      days[i].setText(labels[i]);
      days[i].setChecked(old == null || (old.optInt("days", 127) & (1 << i)) != 0);
      f.addView(days[i]);
    }
    JSONArray presets = hub.store.array("presets");
    List<String> actions = new ArrayList<>();
    actions.add("Desligar");
    actions.add("Ligar com os ajustes salvos");
    for (int i = 0; i < presets.length(); i++)
      actions.add(presets.optJSONObject(i).optString("name"));
    Spinner action = new Spinner(this);
    action.setAdapter(
        new ArrayAdapter<String>(this, android.R.layout.simple_spinner_dropdown_item, actions));
    action.setSelection(old != null && Light.from(old.optJSONObject("light")).power ? 1 : 0);
    add(f, action, 8);
    AlertDialog d =
        dialog(
            "Editar rotina",
            f,
            "Salvar",
            () -> {
              try {
                requireName(name);
                String t = time.getText().toString();
                if (!t.matches("([01][0-9]|2[0-3]):[0-5][0-9]"))
                  throw new Exception("Use HH:mm, por exemplo 22:30");
                int mask = 0;
                for (int i = 0; i < 7; i++) if (days[i].isChecked()) mask |= 1 << i;
                if (mask == 0) throw new Exception("Selecione pelo menos um dia");
                int selected = action.getSelectedItemPosition();
                Light l =
                    selected >= 2
                        ? Light.from(presets.getJSONObject(selected - 2).getJSONObject("light"))
                        : selected == 1 && old != null
                            ? Light.from(old.optJSONObject("light"))
                            : hub.display();
                l.power = selected != 0;
                JSONObject r =
                    new JSONObject()
                        .put("name", name.getText().toString().trim())
                        .put("hour", Integer.parseInt(t.substring(0, 2)))
                        .put("minute", Integer.parseInt(t.substring(3)))
                        .put("days", mask)
                        .put("enabled", enabled.isChecked())
                        .put("light", l.json());
                if (index >= 0) a.put(index, r);
                else {
                  if (a.length() >= 128) throw new Exception("Limite de rotinas atingido");
                  a.put(r);
                }
                hub.store.config.put("routines", a);
                save();
                Routines.schedule(this);
                showPage("Rotinas");
              } catch (Exception e) {
                throw new IllegalArgumentException(e.getMessage());
              }
            });
    if (index >= 0)
      add(
          f,
          button(
              "Excluir rotina",
              () -> {
                delete("routines", index);
                d.dismiss();
              }),
          14);
  }

  void settings() {
    title("Ajustes", "LightControl · controle direto pela rede local");
    LinearLayout c = card();
    EditText name = field(c, "Nome da luz", hub.store.config.optString("name", "Quarto"), 1);
    add(
        c,
        button(
            "Salvar nome",
            () -> {
              try {
                requireName(name);
                hub.store.config.put("name", name.getText().toString().trim());
                save();
                toast("Nome salvo");
              } catch (Exception e) {
                toast(e.getMessage());
              }
            }),
        8);
    add(body, c, 0);
    EditText transition =
        field(
            c,
            "Transição dos presets (segundos, 0–60)",
            (hub.store.config.optInt("transition_ms", 0) / 1000) + "",
            InputType.TYPE_CLASS_NUMBER);
    add(
        c,
        button(
            "Salvar transição",
            () -> {
              try {
                hub.store.config.put("transition_ms", number(transition, 0, 60) * 1000);
                save();
                toast("Transição salva");
              } catch (Exception e) {
                toast(e.getMessage());
              }
            }),
        8);
    LinearLayout connection = card();
    connection.addView(text("Conexão local", 20, TEXT));
    EditText
        ip =
            field(
                connection,
                "IP da lâmpada",
                hub.store.config.optString("last_ip"),
                InputType.TYPE_CLASS_PHONE),
        id = field(connection, "ID do dispositivo", "", 1),
        key =
            field(
                connection,
                "Nova local_key (vazio mantém a atual)",
                "",
                InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
    id.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO);
    key.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO);
    key.setSaveEnabled(false);
    id.setSaveEnabled(false);
    hub.io.execute(
        () -> {
          try {
            JSONObject creds = hub.store.credentials();
            ui.post(
                () -> {
                  if (creds != null && !isFinishing()) id.setText(creds.optString("id"));
                });
          } catch (Exception e) {
            ui.post(() -> toast("Reimporte a credencial protegida"));
          }
        });
    add(
        connection,
        button(
            "Salvar e conectar",
            () -> {
              String newIp = ip.getText().toString().trim(),
                  newId = id.getText().toString().trim(),
                  newKey = key.getText().toString();
              hub.io.execute(
                  () -> {
                    try {
                      JSONObject previous = hub.store.credentials();
                      String secret =
                          newKey.isEmpty() && previous != null ? previous.getString("key") : newKey;
                      hub.store.credentials(
                          new JSONObject().put("id", newId).put("ip", newIp).put("key", secret));
                      ui.post(
                          () -> {
                            key.setText("");
                            toast("Credencial protegida salva");
                            hub.reconnect();
                          });
                    } catch (Exception e) {
                      ui.post(() -> toast(e.getMessage()));
                    }
                  });
            }),
        12);
    add(connection, button("Importar chave do PC…", () -> chooseFile(60)), 10);
    add(connection, button("Reconectar", () -> hub.reconnect()), 10);
    add(connection, button("Descobrir na rede local", () -> hub.discover()), 10);
    add(body, connection, 16);
    LinearLayout controls = card();
    controls.addView(text("Device Control", 20, TEXT));
    add(
        controls,
        text(
            "Samsung: painel rápido → Controle de aparelhos → gerenciar apps → LightControl."
                + " Selecione a luz para controlar energia e brilho sem abrir o app.",
            13,
            MUTED),
        10);
    add(
        controls,
        button(
            "Como adicionar a luz",
            () ->
                new AlertDialog.Builder(this)
                    .setTitle("Device Control / Samsung")
                    .setMessage(
                        "Abra o painel rápido do celular e toque em Controle de aparelhos (ou"
                            + " Device control). No menu de provedores, habilite LightControl e"
                            + " marque a luz. A localização varia com a versão do Android/One UI.")
                    .setPositiveButton("Entendi", null)
                    .show()),
        12);
    add(body, controls, 16);
    if (hub.dndCapable) {
      Switch dnd = new Switch(this);
      dnd.setText("Não perturbe após falta de energia");
      dnd.setChecked(hub.dnd);
      dnd.setTextColor(TEXT);
      dnd.setOnCheckedChangeListener(
          (b, on) -> hub.command(() -> new JSONObject().put("34", on), null));
      add(body, dnd, 18);
    }
    add(
        body,
        text(
            "Tuya 3.5 · TCP 6668 · sem nuvem\n"
                + "A chave fica cifrada neste aparelho. A lâmpada pode limitar conexões simultâneas"
                + " com o PC.",
            12,
            MUTED),
        20);
  }

  void chooseFile(int request) {
    Intent i =
        new Intent(Intent.ACTION_OPEN_DOCUMENT)
            .setType("*/*")
            .addCategory(Intent.CATEGORY_OPENABLE);
    startActivityForResult(i, request);
  }

  @Override
  protected void onActivityResult(int request, int result, Intent data) {
    super.onActivityResult(request, result, data);
    if (request != 60 || result != RESULT_OK || data == null || data.getData() == null) return;
    android.net.Uri uri = data.getData();
    LinearLayout f = form();
    f.addView(text("Arquivo de transferência protegido gerado pelo PC.", 13, MUTED));
    EditText password =
        field(
            f,
            "Senha da transferência",
            "",
            InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
    password.setSaveEnabled(false);
    dialog(
        "Importar chave do PC",
        f,
        "Importar",
        () -> {
          char[] secret = password.getText().toString().toCharArray();
          password.setText("");
          hub.io.execute(
              () -> {
                try (InputStream in = getContentResolver().openInputStream(uri);
                    ByteArrayOutputStream out = new ByteArrayOutputStream()) {
                  if (in == null) throw new IOException("Não foi possível abrir o arquivo");
                  byte[] b = new byte[4096];
                  int n;
                  while ((n = in.read(b)) != -1) {
                    if (out.size() + n > 16384) throw new IOException("Arquivo muito grande");
                    out.write(b, 0, n);
                  }
                  hub.store.importEncrypted(out.toByteArray(), secret);
                  ui.post(
                      () -> {
                        toast("Chave importada e protegida");
                        hub.reconnect();
                        showPage("Ajustes");
                      });
                } catch (Exception e) {
                  ui.post(() -> toast("Não foi possível importar. Confira o arquivo e a senha."));
                } finally {
                  Arrays.fill(secret, '\0');
                }
              });
        });
  }

  void requireName(EditText name) {
    if (name.getText().toString().trim().isEmpty() || name.length() > 64)
      throw new IllegalArgumentException("Use um nome de 1 a 64 caracteres");
  }

  int number(EditText field, int min, int max) {
    try {
      int n = Integer.parseInt(field.getText().toString());
      if (n < min || n > max) throw new NumberFormatException();
      return n;
    } catch (Exception e) {
      throw new IllegalArgumentException("Use um valor entre " + min + " e " + max);
    }
  }

  void delete(String key, int index) {
    new AlertDialog.Builder(this)
        .setTitle("Excluir este item?")
        .setPositiveButton(
            "Excluir",
            (d, w) -> {
              hub.store.array(key).remove(index);
              save();
              if (key.equals("routines")) Routines.schedule(this);
              showPage(page);
            })
        .setNegativeButton("Cancelar", null)
        .show();
  }

  void save() {
    try {
      hub.store.save();
    } catch (Exception e) {
      throw new IllegalStateException("Não foi possível salvar");
    }
  }

  void toast(String s) {
    Toast.makeText(this, s == null ? "Operação não concluída" : s, Toast.LENGTH_LONG).show();
  }

  @Override
  public void onBackPressed() {
    if (!page.equals("Luz")) showPage("Luz");
    else super.onBackPressed();
  }
}
