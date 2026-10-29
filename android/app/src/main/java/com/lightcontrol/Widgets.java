package com.lightcontrol;

import android.content.Context;
import android.graphics.*;
import android.os.Bundle;
import android.view.*;
import android.view.accessibility.*;
import java.util.function.*;

final class Widgets {
  static final int BG = 0xff101311,
      CARD = 0xff191e1a,
      TILE = 0xff242b25,
      TEXT = 0xfff1f2e9,
      MUTED = 0xff98a497,
      ACCENT = 0xffd4e4b8;

  static int mix(int a, int b, float t) {
    return Color.rgb(
        (int) (Color.red(a) + (Color.red(b) - Color.red(a)) * t),
        (int) (Color.green(a) + (Color.green(b) - Color.green(a)) * t),
        (int) (Color.blue(a) + (Color.blue(b) - Color.blue(a)) * t));
  }

  static final class Slider extends View {
    final Paint p = new Paint(3);
    final RectF r = new RectF();
    int value, min, max;
    int[] colors;
    String label;
    BiConsumer<Integer, Boolean> change;
    boolean dragging;

    Slider(
        Context c,
        String label,
        int min,
        int max,
        int value,
        int[] colors,
        BiConsumer<Integer, Boolean> change) {
      super(c);
      this.label = label;
      this.min = min;
      this.max = max;
      this.value = value;
      this.colors = colors;
      this.change = change;
      setFocusable(true);
      setClickable(true);
      setContentDescription(label);
      setMinimumHeight(dp(48));
    }

    int dp(float n) {
      return Math.round(n * getResources().getDisplayMetrics().density);
    }

    @Override
    protected void onMeasure(int w, int h) {
      setMeasuredDimension(
          MeasureSpec.getSize(w),
          MeasureSpec.getMode(h) == MeasureSpec.EXACTLY ? MeasureSpec.getSize(h) : dp(48));
    }

    @Override
    protected void onDraw(Canvas c) {
      float top = getHeight() / 2f - dp(13);
      r.set(dp(2), top, getWidth() - dp(2), top + dp(26));
      p.setShader(new LinearGradient(r.left, 0, r.right, 0, colors, null, Shader.TileMode.CLAMP));
      c.drawRoundRect(r, dp(14), dp(14), p);
      p.setShader(null);
      if (label.equals("Brilho")) {
        p.setColor(0x607f8e6d);
        p.setStrokeWidth(dp(1));
        for (int i = 1; i < 12; i++) {
          float x = r.left + r.width() * i / 12;
          c.drawLine(x, r.centerY() - dp(4), x, r.centerY() + dp(4), p);
        }
      }
      float x = dp(12) + (getWidth() - dp(24)) * (value - min) / (float) (max - min);
      p.setShader(
          new LinearGradient(
              x - dp(7),
              0,
              x + dp(7),
              0,
              new int[] {0xffaab899, 0xffe0e8d2},
              null,
              Shader.TileMode.CLAMP));
      r.set(x - dp(9), getHeight() / 2f - dp(11), x + dp(9), getHeight() / 2f + dp(11));
      c.drawRoundRect(r, dp(8), dp(8), p);
      p.setShader(null);
      p.setColor(0xff7f8e6d);
      p.setStrokeWidth(dp(1));
      c.drawLine(x - dp(2), r.top + dp(5), x - dp(2), r.bottom - dp(5), p);
      c.drawLine(x + dp(2), r.top + dp(5), x + dp(2), r.bottom - dp(5), p);
    }

    void update(int n) {
      if (!dragging) {
        value = Light.clamp(n, min, max);
        invalidate();
      }
    }

    void set(int n, boolean end) {
      value = Light.clamp(n, min, max);
      invalidate();
      if (change != null) change.accept(value, end);
      sendAccessibilityEvent(AccessibilityEvent.TYPE_VIEW_SELECTED);
    }

    @Override
    public boolean onTouchEvent(MotionEvent e) {
      switch (e.getActionMasked()) {
        case MotionEvent.ACTION_DOWN:
          dragging = true;
          getParent().requestDisallowInterceptTouchEvent(true);
        case MotionEvent.ACTION_MOVE:
          set(
              Math.round(
                  min
                      + (max - min)
                          * Math.max(
                              0,
                              Math.min(1, (e.getX() - dp(12)) / Math.max(1, getWidth() - dp(24))))),
              false);
          return true;
        case MotionEvent.ACTION_UP:
          dragging = false;
          set(value, true);
          performClick();
          return true;
        case MotionEvent.ACTION_CANCEL:
          dragging = false;
          set(value, true);
          return true;
      }
      return false;
    }

    @Override
    public boolean performClick() {
      super.performClick();
      return true;
    }

    @Override
    public boolean onKeyDown(int key, KeyEvent e) {
      if (key == KeyEvent.KEYCODE_DPAD_LEFT || key == KeyEvent.KEYCODE_DPAD_RIGHT) {
        set(value + (key == KeyEvent.KEYCODE_DPAD_RIGHT ? 1 : -1), true);
        return true;
      }
      return super.onKeyDown(key, e);
    }

    @Override
    public void onInitializeAccessibilityNodeInfo(AccessibilityNodeInfo info) {
      super.onInitializeAccessibilityNodeInfo(info);
      info.setClassName("android.widget.SeekBar");
      info.setRangeInfo(
          AccessibilityNodeInfo.RangeInfo.obtain(
              AccessibilityNodeInfo.RangeInfo.RANGE_TYPE_INT, min, max, value));
      info.setStateDescription(value + (max == 100 ? "%" : ""));
      info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS);
      info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
      info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
    }

    @Override
    public boolean performAccessibilityAction(int a, Bundle b) {
      if (a == AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.getId() && b != null) {
        set(Math.round(b.getFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE)), true);
        return true;
      }
      if (a == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD
          || a == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD) {
        set(value + (a == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD ? 5 : -5), true);
        return true;
      }
      return super.performAccessibilityAction(a, b);
    }
  }

  static final class Bulb extends View {
    final Paint p = new Paint(3);
    final RectF r = new RectF();
    final Rect visible = new Rect();
    private Shader halo;
    private int haloTint;
    Light light = new Light();
    boolean known;

    Bulb(Context c) {
      super(c);
      setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
    }

    @Override
    protected void onDraw(Canvas canvas) {
      float scale = getResources().getDisplayMetrics().density;
      canvas.save();
      canvas.translate(getWidth() / 2f, getHeight() / 2f - 5 * scale);
      canvas.scale(scale, scale);
      boolean on = known && light.power;
      int tint =
          light.mode.equals("colour")
              ? light.rgb()
              : mix(0xffffb86b, 0xffb8dbe8, light.temperature / 100f);
      boolean animate = on && android.animation.ValueAnimator.areAnimatorsEnabled();
      float phase = android.os.SystemClock.uptimeMillis() / 1000f;
      float energy =
          on
              ? (0.45f + light.level() / 100f * 0.55f)
                  * (animate ? 0.88f + (float) Math.sin(phase) * 0.12f : 1f)
              : 0;
      if (on) {
        if (halo == null || haloTint != tint) {
          haloTint = tint;
          halo =
              new RadialGradient(
                  0,
                  -3,
                  78,
                  new int[] {
                    (tint & 0xffffff) | 0x35000000, (tint & 0xffffff) | 0x17000000, tint & 0xffffff
                  },
                  new float[] {0, .45f, 1},
                  Shader.TileMode.CLAMP);
        }
        p.setShader(halo);
        p.setAlpha(Math.round(255 * energy));
        canvas.drawOval(-78, -74, 78, 68, p);
        p.setAlpha(255);
        p.setShader(null);
      }
      int glass = mix(0xff414b40, tint, .28f + energy * .62f);
      p.setColor(glass);
      canvas.drawOval(-29, -38, 29, 26, p);
      r.set(-17, 9, 17, 34);
      canvas.drawRoundRect(r, 9, 9, p);
      p.setColor(mix(glass, 0xffffffff, .14f + energy * .2f));
      canvas.drawOval(-23, -31, 11, -1, p);
      p.setStrokeCap(Paint.Cap.ROUND);
      p.setStrokeWidth(2.5f);
      p.setColor(0xfff2f4df);
      canvas.drawLine(-17, -19, -13, -26, p);
      p.setStrokeWidth(1);
      p.setColor(mix(glass, 0xffffffff, .45f));
      canvas.drawLine(-8, 19, -4, -1, p);
      canvas.drawLine(8, 19, 4, -1, p);
      canvas.drawLine(-4, -1, 4, -1, p);
      p.setShader(
          new LinearGradient(
              -15,
              0,
              15,
              0,
              new int[] {0xff7c8a77, 0xff465044, 0xff84927e},
              null,
              Shader.TileMode.CLAMP));
      r.set(-15, 30, 15, 39);
      canvas.drawRoundRect(r, 3, 3, p);
      p.setShader(null);
      p.setColor(0xff6b7864);
      r.set(-12, 40, 12, 44);
      canvas.drawRoundRect(r, 2, 2, p);
      p.setColor(0xff434e40);
      r.set(-7, 46, 7, 50);
      canvas.drawRoundRect(r, 2, 2, p);
      if (animate) {
        p.setColor(mix(CARD, tint, energy * .45f));
        for (int i = 0; i < 5; i++) {
          float angle = i * 1.2566f + phase * .22f;
          canvas.drawCircle(
              (float) Math.cos(angle) * 59, -6 + (float) Math.sin(angle) * 51, 1.4f, p);
        }
      }
      canvas.restore();
      if (animate && getWindowVisibility() == VISIBLE && isShown() && getGlobalVisibleRect(visible))
        postInvalidateDelayed(50);
    }
  }
}
