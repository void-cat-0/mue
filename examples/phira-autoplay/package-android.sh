#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'HELP'
用法：package-android.sh [--so FILE] [-o APK] [--work-dir DIR] [--notices-dir DIR]

未指定 --so 时，用 NDK r27 为 arm64-v8a / API 26 构建 phira-autoplay-android。
--so FILE          已构建的 Android cdylib（跳过 Cargo）。
-o, --output APK   输出路径，默认是工作目录里的 phira-autoplay-debug.apk。
--work-dir DIR     暂存目录根，默认 ${TMPDIR:-/tmp}/phira-autoplay-android。
--notices-dir DIR  追加到 APK assets/licenses/ 的第三方依赖声明。

环境变量：ANDROID_HOME / ANDROID_SDK_ROOT、ANDROID_NDK_HOME / ANDROID_NDK_ROOT、
ANDROID_BUILD_TOOLS（版本，默认 34.0.0）、ANDROID_COMPILE_SDK（默认 35）、
ANDROID_ELF_PAGE_SIZE（4096 / 16384 / 65536，默认 16384）、JAVA_HOME
（仅有 Build Tools 34 自带的 D8 时请使用 JDK 17）、
CARGO_TARGET_DIR（默认工作目录里的 target）、
PHIRA_ANDROID_SO / PHIRA_ANDROID_APK（等同于 --so / --output）、
ANDROID_DEBUG_KEYSTORE（可选自备标准 debug 密钥库，默认在工作目录生成并复用）。
将 charts/ 下的 ManaBlast.pez 和 ExoplanetaryMirage.かめりあ-AT.pez 打入 APK。
charts/ 或仓库根目录的本地 note 材质和打击音效是可选的。
HELP
}

fail() { printf 'Android 打包失败：%s\n' "$*" >&2; exit 1; }

native_so="${PHIRA_ANDROID_SO:-}"
output="${PHIRA_ANDROID_APK:-}"
work_dir="${TMPDIR:-/tmp}/phira-autoplay-android"
notices_dir=""
while (($#)); do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --so|-o|--output|--work-dir|--notices-dir)
            (($# >= 2)) && [[ -n "$2" ]] || fail "$1 缺少路径"
            case "$1" in
                --so) native_so="$2" ;;
                -o|--output) output="$2" ;;
                --work-dir) work_dir="$2" ;;
                --notices-dir) notices_dir="$2" ;;
            esac
            shift 2 ;;
        *) fail "未知参数：$1（见 --help）" ;;
    esac
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd -- "$script_dir/../.." && pwd)"
miniquad="$repo/../miniquad"
abi=arm64-v8a
api=26
example=phira-autoplay-android
package=com.mue.phiraautoplay
library=phira_autoplay_android
elf_page_size="${ANDROID_ELF_PAGE_SIZE:-16384}"
case "$elf_page_size" in
    4096|16384|65536) ;;
    *) fail 'ANDROID_ELF_PAGE_SIZE 应为 4096、16384 或 65536' ;;
esac

if [[ -n "$native_so" ]]; then
    [[ -f "$native_so" ]] || fail "找不到预编译 .so：$native_so"
fi
sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[[ -d "$sdk" ]] || fail '请设置 ANDROID_HOME 或 ANDROID_SDK_ROOT'
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [[ -z "$ndk" ]]; then
    for candidate in "$sdk"/ndk/27.*; do
        [[ ! -f "$candidate/source.properties" ]] || ndk="$candidate"
    done
fi
[[ -n "$ndk" ]] && grep -Eq '^Pkg.Revision[[:space:]]*=[[:space:]]*27\.' "$ndk/source.properties" \
    || fail '请设置 ANDROID_NDK_HOME 指向 NDK r27'
# The LLVM tools are in the sole host toolchain shipped in the NDK.
llvm_bins=("$ndk"/toolchains/llvm/prebuilt/*/bin)
llvm_bin="${llvm_bins[0]}"
build_tools="$sdk/build-tools/${ANDROID_BUILD_TOOLS:-34.0.0}"
android_jar="$sdk/platforms/android-${ANDROID_COMPILE_SDK:-35}/android.jar"
[[ -f "$android_jar" ]] || fail "找不到 Android platform：$android_jar"
for tool in "$build_tools/"{aapt2,zipalign,apksigner} "$llvm_bin/"{llvm-readelf,llvm-strip,llvm-nm}; do
    [[ -x "$tool" ]] || fail "找不到工具：$tool"
done
if [[ -n "${JAVA_HOME:-}" ]]; then
    export PATH="$JAVA_HOME/bin:$PATH"
fi
for tool in java jar javac keytool python3; do
    hash "$tool" || fail "找不到工具：$tool"
done
# Build Tools 34's D8 cannot consume javac 21's unnamed MethodParameters.
# The SDK command-line tools ship a fixed D8; otherwise use the build tools one.
r8_jar="$sdk/cmdline-tools/latest/lib/r8.jar"
if [[ -f "$r8_jar" ]]; then
    d8=(java -cp "$r8_jar" com.android.tools.r8.D8)
else
    [[ -x "$build_tools/d8" ]] || fail "找不到工具：$build_tools/d8"
    d8=("$build_tools/d8")
fi

chart_dir="$script_dir/charts"
mana_source="$chart_dir/ManaBlast.pez"
mirage_source="$chart_dir/ExoplanetaryMirage.かめりあ-AT.pez"
for input in "$mana_source" "$mirage_source" "$miniquad/java/"{MainActivity,QuadNative}.java; do
    [[ -f "$input" ]] || fail "找不到输入：$input"
done

mkdir -p -- "$work_dir"
work_dir="$(cd -- "$work_dir" && pwd)"
output="${output:-$work_dir/phira-autoplay-debug.apk}"
[[ "$output" == *.apk ]] || fail '输出路径必须以 .apk 结尾'
stage="$(mktemp -d "$work_dir/stage.XXXXXXXX")"
trap 'rm -rf -- "$stage"' EXIT

if [[ -z "$native_so" ]]; then
    export ANDROID_NDK_HOME="$(cd -- "$ndk" && pwd)"
    export CARGO_TARGET_DIR="$(realpath -m -- "${CARGO_TARGET_DIR:-$work_dir/target}")"
    (
        cd -- "$repo/examples"
        # Sasa's Oboe uses libc++_shared; link the shader compiler to the same runtime.
        CXXSTDLIB=c++_shared CXXSTDLIB_STATIC= MUE_RENDER_BACKEND=auto \
            cargo ndk -t "$abi" --platform "$api" \
            rustc --locked --release --features phira-autoplay-vulkan-preferred --example "$example" -- \
            -C link-arg=-Wl,--no-undefined \
            -C "link-arg=-Wl,-z,max-page-size=$elf_page_size"
    )
    native_so="$CARGO_TARGET_DIR/aarch64-linux-android/release/examples/lib$library.so"
fi

check_native_library() {
    local file="$1" header alignments alignment
    header="$("$llvm_bin/llvm-readelf" -h "$file")"
    [[ "$header" == *ELF64* && "$header" == *AArch64* && "$header" == *DYN* ]] \
        || fail "不是 arm64-v8a 共享库：$file"
    alignments="$("$llvm_bin/llvm-readelf" -lW "$file" | awk '$1 == "LOAD" { print $NF }')"
    [[ -n "$alignments" ]] || fail "缺少 ELF LOAD 段：$file"
    while IFS= read -r alignment; do
        ((alignment >= elf_page_size)) || fail "$file 的 ELF LOAD 对齐 $alignment 小于 $elf_page_size 字节"
    done <<< "$alignments"
}

check_native_library "$native_so"
dynamic="$("$llvm_bin/llvm-readelf" -d "$native_so")"
exports="$("$llvm_bin/llvm-nm" --dynamic --defined-only --just-symbol-name "$native_so")"
for symbol in JNI_OnLoad quad_main Java_quad_1native_QuadNative_{initializeContext,setChartDirectory,setDiagnosticLogPath,setForceGles,setHostPaused,activityOnCreate}; do
    grep -Fxq "$symbol" <<< "$exports" || fail "原生库缺少导出：$symbol"
done

mkdir -p "$stage"/{assets/licenses,classes,dex,java,lib/arm64-v8a}
media=(ManaBlast.pez ExoplanetaryMirage-AT.pez)
cp -- "$mana_source" "$stage/assets/${media[0]}"
# AAPT2 requires ASCII asset paths; the unchanged PEZ supplies its own title and filenames.
cp -- "$mirage_source" "$stage/assets/${media[1]}"
for sound in click.ogg drag.ogg flick.ogg; do
    for directory in "$chart_dir" "$repo"; do
        if [[ -f "$directory/$sound" ]]; then
            cp -- "$directory/$sound" "$stage/assets/$sound"
            media+=("$sound")
            break
        fi
    done
done
for directory in "$chart_dir" "$repo"; do
    for skin in 'live 测试5.zip' live-skin.zip; do
        if [[ -f "$directory/$skin" ]]; then
            cp -- "$directory/$skin" "$stage/assets/live-skin.zip"
            media+=(live-skin.zip)
            break 2
        fi
    done
done
cp -- "$repo/LICENSE" "$stage/assets/licenses/Mue-MIT.txt"
cp -- "$miniquad/LICENSE-MIT" "$stage/assets/licenses/Miniquad-MIT.txt"
cp -- "$repo/../macroquad/LICENSE-MIT" "$stage/assets/licenses/Macroquad-MIT.txt"
cp -- "$repo/examples/playground/assets/fonts/"OFL-Noto{CJK,LatinGreekCyrillic}.txt "$stage/assets/licenses/"
cp -- "$script_dir/Sasa-notice.txt" "$stage/assets/licenses/"
if [[ -n "$notices_dir" ]]; then
    [[ -d "$notices_dir" ]] || fail "找不到第三方依赖声明目录：$notices_dir"
    cp -R -- "$notices_dir/." "$stage/assets/licenses/"
fi
"$llvm_bin/llvm-strip" --strip-unneeded -o "$stage/lib/$abi/lib$library.so" "$native_so"
if [[ "$dynamic" == *'[libc++_shared.so]'* ]]; then
    cxx_runtime="$llvm_bin/../sysroot/usr/lib/aarch64-linux-android/libc++_shared.so"
    [[ -f "$cxx_runtime" ]] || fail "找不到对应的 NDK C++ 运行时：$cxx_runtime"
    check_native_library "$cxx_runtime"
    cp -- "$cxx_runtime" "$stage/lib/$abi/"
fi

python3 - "$miniquad/java" "$stage/java" "$package" "$library" "${media[@]}" <<'PY'
import json
from pathlib import Path
import re
import sys

src, dst = map(Path, sys.argv[1:3])
package, library, *media = sys.argv[3:]


def inject(source, anchor, replacement):
    if source.count(anchor) != 1:
        raise SystemExit(f"Miniquad Java 模板不匹配：{anchor}")
    return source.replace(anchor, replacement)


activity = (src / 'MainActivity.java').read_text(encoding='utf-8')
activity = activity.replace('TARGET_PACKAGE_NAME', package).replace('LIBRARY_NAME', library)
activity = re.sub(r'^\s*//% [^\n]+\n', '\n', activity, flags=re.MULTILINE)
activity = inject(activity, 'import android.app.Activity;', '''import android.app.Activity;
import android.app.AlarmManager;
import android.app.PendingIntent;
import android.os.Bundle;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import java.io.File;
import java.io.FileOutputStream;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.IOException;
import android.view.Gravity;
import android.widget.Toast;''')
activity = inject(activity, '    private QuadSurface view;', '''    private QuadSurface view;
    // The application context is process-wide, not tied to this Activity.
    private static boolean contextInitialized;

    private static final int REQUEST_DIAGNOSTIC_EXPORT = 4107;
    private static final String DIAGNOSTIC_PREFS = "phira-autoplay-renderer";
    private boolean forceGles;
    private boolean restartRequested;
    private TextView graphicsApi;

    private int dp(float value) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value,
                getResources().getDisplayMetrics());
    }

    private void addDiagnosticControls(ResizingLayout layout) {
        forceGles = getSharedPreferences(DIAGNOSTIC_PREFS, MODE_PRIVATE)
                .getBoolean("force_gles", false);
        LinearLayout controls = new LinearLayout(this);
        controls.setOrientation(LinearLayout.VERTICAL);
        controls.setPadding(dp(5), dp(5), dp(5), dp(5));
        controls.setBackgroundColor(Color.argb(224, 18, 26, 38));

        graphicsApi = new TextView(this);
        graphicsApi.setText("图形 API：初始化中…");
        graphicsApi.setTextColor(Color.WHITE);
        graphicsApi.setTextSize(13);
        graphicsApi.setMaxWidth(dp(244));
        graphicsApi.setPadding(dp(14), dp(6), dp(14), dp(4));
        controls.addView(graphicsApi, new LinearLayout.LayoutParams(-2, -2));

        TextView export = new TextView(this);
        export.setText("导出启动日志");
        export.setTextColor(Color.WHITE);
        export.setTextSize(15);
        export.setPadding(dp(14), dp(10), dp(14), dp(10));
        export.setClickable(true);
        export.setFocusable(true);
        export.setOnClickListener(view -> exportDiagnosticLog());
        controls.addView(export, new LinearLayout.LayoutParams(-2, -2));

        TextView notice = new TextView(this);
        notice.setText("Vulkan 启动失败时，日志会记录可读取的设备 Vulkan API 信息。");
        notice.setTextColor(Color.LTGRAY);
        notice.setTextSize(12);
        notice.setMaxWidth(dp(220));
        notice.setPadding(dp(14), 0, dp(14), dp(6));
        controls.addView(notice, new LinearLayout.LayoutParams(-2, -2));

        TextView renderer = new TextView(this);
        renderer.setText(forceGles ? "自动模式重启" : "GLES 重启");
        renderer.setTextColor(Color.WHITE);
        renderer.setTextSize(15);
        renderer.setPadding(dp(14), dp(10), dp(14), dp(10));
        renderer.setClickable(true);
        renderer.setFocusable(true);
        renderer.setOnClickListener(view -> restartWithGles(!forceGles));
        controls.addView(renderer, new LinearLayout.LayoutParams(-2, -2));

        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(
                -2, -2, Gravity.TOP | Gravity.RIGHT);
        params.setMargins(0, dp(28), dp(12), 0);
        layout.addView(controls, params);
        layout.bringChildToFront(controls);
        controls.setTranslationZ(dp(24));
    }

    public void setGraphicsApi(String status) {
        runOnUiThread(() -> graphicsApi.setText("图形 API：" + status));
    }

    private void exportDiagnosticLog() {
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("text/plain");
        intent.putExtra(Intent.EXTRA_TITLE, "PhiraAutoplay-diagnostics.txt");
        try {
            startActivityForResult(intent, REQUEST_DIAGNOSTIC_EXPORT);
        } catch (android.content.ActivityNotFoundException error) {
            Log.e("SAPP", "No document picker for diagnostic export", error);
            Toast.makeText(this, "无法打开系统文件保存器", Toast.LENGTH_LONG).show();
        }
    }

    public void requestGlesFallback() {
        runOnUiThread(() -> restartWithGles(true));
    }

    private void restartWithGles(boolean useGles) {
        if (restartRequested) return;
        restartRequested = true;
        forceGles = useGles;
        getSharedPreferences(DIAGNOSTIC_PREFS, MODE_PRIVATE).edit()
                .putBoolean("force_gles", useGles).commit();
        String mode = useGles ? "GLES" : "自动 Vulkan / GLES";
        Log.w("SAPP", "Restart requested with renderer mode: " + mode);
        Toast.makeText(this, mode + "，正在重启", Toast.LENGTH_SHORT).show();
        Intent launch = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (launch == null) {
            restartRequested = false;
            return;
        }
        launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        PendingIntent restart = PendingIntent.getActivity(this, 0, launch,
                PendingIntent.FLAG_CANCEL_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        AlarmManager alarms = (AlarmManager) getSystemService(ALARM_SERVICE);
        alarms.setAndAllowWhileIdle(
                AlarmManager.ELAPSED_REALTIME,
                SystemClock.elapsedRealtime() + 900,
                restart);
        finishAndRemoveTask();
        new Handler(Looper.getMainLooper()).postDelayed(
                () -> android.os.Process.killProcess(android.os.Process.myPid()), 150);
    }

    private void handleDiagnosticExport(int resultCode, Intent data) {
        if (resultCode != RESULT_OK || data == null || data.getData() == null) return;
        File log = new File(getFilesDir(), "phira-autoplay.log");
        try (InputStream input = new FileInputStream(log);
             OutputStream output = getContentResolver().openOutputStream(data.getData(), "wt")) {
            if (output == null) throw new IOException("Could not open the selected destination");
            byte[] buffer = new byte[8192];
            int count;
            while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
            output.flush();
            Toast.makeText(this, "诊断日志已保存", Toast.LENGTH_LONG).show();
        } catch (IOException error) {
            Log.e("SAPP", "Could not export diagnostic log", error);
            Toast.makeText(this, "日志导出失败", Toast.LENGTH_LONG).show();
        }
    }

    private File copyMedia() {
        File charts = new File(getFilesDir(), "charts");
        if (!charts.isDirectory() && !charts.mkdirs()) {
            throw new IllegalStateException("Cannot create chart directory: " + charts);
        }
        byte[] buffer = new byte[64 * 1024];
        for (String name : new String[] {MEDIA_FILES}) {
            File chart = new File(charts, name);
            File temp = new File(charts, name + ".tmp");
            try {
                try (InputStream in = getAssets().open(name);
                     FileOutputStream out = new FileOutputStream(temp)) {
                    int count;
                    while ((count = in.read(buffer)) != -1) {
                        out.write(buffer, 0, count);
                    }
                }
                // Both paths are on the same filesystem; replace the complete file at once.
                if (!temp.renameTo(chart)) {
                    throw new IOException("Cannot rename " + temp + " to " + chart);
                }
            } catch (IOException e) {
                temp.delete();
                throw new IllegalStateException("Cannot copy bundled asset to " + chart, e);
            }
        }
        return charts;
    }

    public void requestFinish() {
        runOnUiThread(this::finishAndRemoveTask);
    }''')
activity = inject(activity, 'MEDIA_FILES', ', '.join(map(json.dumps, media)))
activity = inject(activity, '    protected void onActivityResult(int requestCode, int resultCode, Intent data) {', '''    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQUEST_DIAGNOSTIC_EXPORT) {
            handleDiagnosticExport(resultCode, data);
            return;
        }''')

activity = inject(activity, 'layout.addView(sHiddenEditText);\n        setContentView(layout);', '''layout.addView(sHiddenEditText);
        setContentView(layout);
        addDiagnosticControls(layout);''')
activity = inject(activity, '        super.onCreate(savedInstanceState);', '''        super.onCreate(savedInstanceState);

        QuadNative.setDiagnosticLogPath(new File(getFilesDir(), "phira-autoplay.log").getAbsolutePath());
        if (!contextInitialized) {
            QuadNative.initializeContext(getApplicationContext());
            contextInitialized = true;
        }
        boolean forceGles = getSharedPreferences("phira-autoplay-renderer", MODE_PRIVATE)
                .getBoolean("force_gles", false);
        QuadNative.setForceGles(forceGles);
        QuadNative.setChartDirectory(this, copyMedia().getAbsolutePath());''')
for callback, paused in [('Resume', 'false'), ('Pause', 'true')]:
    anchor = f'        QuadNative.activityOn{callback}();'
    activity = inject(activity, anchor, f'        QuadNative.setHostPaused({paused});\n{anchor}')
activity = inject(activity, '''        Log.w("SAPP", "onBackPressed");

        // TODO: here is the place to handle request_quit/order_quit/cancel_quit

        super.onBackPressed();''', '''        QuadNative.surfaceOnKeyDown(KeyEvent.KEYCODE_BACK);
        QuadNative.surfaceOnKeyUp(KeyEvent.KEYCODE_BACK);''')
activity = inject(activity, '    public boolean onKey(View v, int keyCode, KeyEvent event) {', '''    public boolean onKey(View v, int keyCode, KeyEvent event) {
        // Let Activity dispatch Back through onBackPressed, including at the root.
        if (keyCode == KeyEvent.KEYCODE_BACK) return false;''')
(dst / 'MainActivity.java').write_text(activity, encoding='utf-8')

native = (src / 'QuadNative.java').read_text(encoding='utf-8')
native = inject(native, 'public class QuadNative {', '''public class QuadNative {
    public native static void initializeContext(Object context);
    public native static void setDiagnosticLogPath(String path);
    public native static void setForceGles(boolean forceGles);
    public native static void setChartDirectory(Object activity, String path);
    public native static void setHostPaused(boolean paused);
''')
(dst / 'QuadNative.java').write_text(native, encoding='utf-8')
PY

javac -encoding UTF-8 --release 8 -classpath "$android_jar" -d "$stage/classes" "$stage/java/"*.java
jar cf "$stage/classes.jar" -C "$stage/classes" .
"${d8[@]}" --min-api "$api" --lib "$android_jar" --output "$stage/dex" "$stage/classes.jar"
"$build_tools/aapt2" link --manifest "$script_dir/android/AndroidManifest.xml" -I "$android_jar" \
    -A "$stage/assets" -0 pez -0 ogg -0 zip --debug-mode -o "$stage/unsigned.apk"

# A compressed .so is extracted at install, so the older zipalign's 4 KiB
# native-library ZIP alignment isn't used. Its ELF segments still need 16 KiB.
python3 - "$stage" <<'PY'
from pathlib import Path
from shutil import copyfileobj
import sys
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

stage = Path(sys.argv[1])
files = [(stage / 'dex/classes.dex', 'classes.dex')]
files.extend((lib, lib.relative_to(stage).as_posix()) for lib in sorted(stage.glob('lib/*/*.so')))
with ZipFile(stage / 'unsigned.apk', 'a') as apk:
    for source, member in files:
        entry = ZipInfo(member)  # Fixed ZIP timestamp, as with the aapt2 entries.
        entry.compress_type = ZIP_DEFLATED
        with source.open('rb') as src, apk.open(entry, 'w') as dst:
            copyfileobj(src, dst)
PY
"$build_tools/zipalign" -f 4 "$stage/unsigned.apk" "$stage/aligned.apk"

keystore="${ANDROID_DEBUG_KEYSTORE:-$work_dir/debug.keystore}"
if [[ ! -f "$keystore" && -z "${ANDROID_DEBUG_KEYSTORE:-}" ]]; then
    keytool -genkeypair -keystore "$stage/debug.keystore" -storetype PKCS12 \
        -storepass android -keypass android -alias androiddebugkey \
        -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=Android Debug,O=Android,C=US'
    # Publish without overwriting if another build generated the shared key first.
    ln -- "$stage/debug.keystore" "$keystore" || [[ -f "$keystore" ]]
fi
[[ -f "$keystore" ]] || fail "找不到 debug 密钥库：$keystore"
"$build_tools/apksigner" sign --ks "$keystore" --ks-key-alias androiddebugkey \
    --ks-pass pass:android --key-pass pass:android --v4-signing-enabled false \
    --out "$stage/signed.apk" "$stage/aligned.apk"
"$build_tools/apksigner" verify --verbose "$stage/signed.apk"
"$build_tools/zipalign" -c 4 "$stage/signed.apk"

mkdir -p -- "$(dirname -- "$output")"
install -T -m 644 -- "$stage/signed.apk" "$output"
printf 'APK: %s\n' "$(realpath -- "$output")"
