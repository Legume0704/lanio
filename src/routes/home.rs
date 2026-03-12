use crate::config::Config;
use axum::{extract::State, response::Html};
use std::sync::Arc;

pub async fn home_handler(State(config): State<Arc<Config>>) -> Html<String> {
    if config.auth_token.is_some() {
        Html(render_auth_page(&config))
    } else {
        Html(render_main_page(&config))
    }
}

fn base_url_for_display(config: &Config) -> String {
    config
        .public_url
        .as_ref()
        .or(config.base_url.as_ref())
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_default()
}

fn render_auth_page(config: &Config) -> String {
    let server_base = base_url_for_display(config);

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lanio</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: #1a1a2e;
      color: #e0e0e0;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 2rem;
    }}
    .card {{
      background: #16213e;
      border-radius: 12px;
      padding: 2.5rem;
      max-width: 600px;
      width: 100%;
      box-shadow: 0 8px 32px rgba(0,0,0,0.4);
    }}
    h1 {{ font-size: 1.8rem; margin-bottom: 0.5rem; color: #fff; }}
    .subtitle {{ color: #888; margin-bottom: 2rem; }}
    h2 {{ font-size: 1rem; text-transform: uppercase; letter-spacing: 0.05em; color: #888; margin-bottom: 0.75rem; }}
    .url-box {{
      background: #0f3460;
      border-radius: 8px;
      padding: 0.75rem 1rem;
      font-family: monospace;
      font-size: 0.9rem;
      word-break: break-all;
      margin-bottom: 1.5rem;
      color: #a8d8ea;
    }}
    .steps {{ margin-bottom: 2rem; }}
    .step {{
      display: flex;
      gap: 1rem;
      margin-bottom: 0.75rem;
      align-items: flex-start;
    }}
    .step-num {{
      background: #533483;
      color: #fff;
      border-radius: 50%;
      width: 1.6rem;
      height: 1.6rem;
      flex-shrink: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.8rem;
      font-weight: bold;
    }}
    .step-text {{ padding-top: 0.15rem; line-height: 1.5; }}
    .btn {{
      display: inline-block;
      background: #533483;
      color: #fff;
      text-decoration: none;
      padding: 0.75rem 1.5rem;
      border-radius: 8px;
      font-size: 1rem;
      font-weight: 600;
      transition: background 0.2s;
      cursor: pointer;
      border: none;
    }}
    .btn:hover {{ background: #6a42a8; }}
    .input-row {{
      display: flex;
      gap: 0.75rem;
      margin-bottom: 1rem;
    }}
    .input-row input {{
      flex: 1;
      background: #0f3460;
      border: 1px solid #1a4a80;
      border-radius: 8px;
      padding: 0.75rem 1rem;
      color: #e0e0e0;
      font-size: 1rem;
      outline: none;
    }}
    .input-row input:focus {{ border-color: #533483; }}
    .error-msg {{
      color: #e05555;
      font-size: 0.9rem;
      margin-bottom: 1rem;
      display: none;
    }}
    #manifest-section {{ display: none; }}
  </style>
</head>
<body>
  <div class="card">
    <h1>Lanio</h1>
    <p class="subtitle">Stream your local media files in Stremio</p>

    <div id="auth-section">
      <h2>Password Required</h2>
      <div class="input-row">
        <input type="password" id="password-input" placeholder="Enter password" />
        <button class="btn" onclick="unlock()">Unlock</button>
      </div>
      <p class="error-msg" id="auth-error">Incorrect password</p>
    </div>

    <div id="manifest-section">
      <h2>Manifest URL</h2>
      <div class="url-box" id="manifest-url"></div>

      <h2>How to Install</h2>
      <div class="steps">
        <div class="step">
          <div class="step-num">1</div>
          <div class="step-text">Open Stremio and click the <strong>puzzle icon</strong> (Add-ons) in the top right</div>
        </div>
        <div class="step">
          <div class="step-num">2</div>
          <div class="step-text">Click <strong>Community Add-ons</strong> at the bottom of the page</div>
        </div>
        <div class="step">
          <div class="step-num">3</div>
          <div class="step-text">Paste the manifest URL above and click <strong>Install</strong></div>
        </div>
      </div>

      <a id="install-link" href="about:blank" class="btn">Install in Stremio Web</a>
    </div>
  </div>

  <script>
    const SERVER_BASE = "{server_base}";

    document.getElementById('password-input').addEventListener('keydown', function(e) {{
      if (e.key === 'Enter') unlock();
    }});

    async function computeToken(password) {{
      const enc = new TextEncoder();
      const [h1, h2] = await Promise.all([
        crypto.subtle.digest('SHA-512', enc.encode('lanio_auth_a:' + password)),
        crypto.subtle.digest('SHA-512', enc.encode('lanio_auth_b:' + password)),
      ]);
      const hex = buf => Array.from(new Uint8Array(buf))
        .map(b => b.toString(16).padStart(2, '0')).join('');
      return hex(h1) + hex(h2);
    }}

    async function unlock() {{
      const password = document.getElementById('password-input').value;
      if (!password) return;

      const token = await computeToken(password);
      const base = SERVER_BASE || window.location.origin;
      const manifestUrl = base + '/' + token + '/manifest.json';

      try {{
        const resp = await fetch(manifestUrl);
        if (!resp.ok) throw new Error('bad status');
        await resp.json();

        document.getElementById('manifest-url').textContent = manifestUrl;
        document.getElementById('install-link').href =
          'https://web.stremio.com/#/addons?addon=' + encodeURIComponent(manifestUrl);
        document.getElementById('auth-section').style.display = 'none';
        document.getElementById('manifest-section').style.display = 'block';
        document.getElementById('auth-error').style.display = 'none';
      }} catch (_) {{
        document.getElementById('auth-error').style.display = 'block';
      }}
    }}
  </script>
</body>
</html>"##,
        server_base = server_base,
    )
}

fn render_main_page(config: &Config) -> String {
    let manifest_url = config
        .public_url
        .as_ref()
        .or(config.base_url.as_ref())
        .map(|s| format!("{}/manifest.json", s.trim_end_matches('/')))
        .unwrap_or_else(|| format!("http://localhost:{}/manifest.json", config.port));

    let install_url = format!(
        "https://web.stremio.com/#/addons?addon={}",
        urlencoding::encode(&manifest_url)
    );

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Lanio</title>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      background: #1a1a2e;
      color: #e0e0e0;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 2rem;
    }}
    .card {{
      background: #16213e;
      border-radius: 12px;
      padding: 2.5rem;
      max-width: 600px;
      width: 100%;
      box-shadow: 0 8px 32px rgba(0,0,0,0.4);
    }}
    h1 {{ font-size: 1.8rem; margin-bottom: 0.5rem; color: #fff; }}
    .subtitle {{ color: #888; margin-bottom: 2rem; }}
    h2 {{ font-size: 1rem; text-transform: uppercase; letter-spacing: 0.05em; color: #888; margin-bottom: 0.75rem; }}
    .url-box {{
      background: #0f3460;
      border-radius: 8px;
      padding: 0.75rem 1rem;
      font-family: monospace;
      font-size: 0.9rem;
      word-break: break-all;
      margin-bottom: 1.5rem;
      color: #a8d8ea;
    }}
    .steps {{ margin-bottom: 2rem; }}
    .step {{
      display: flex;
      gap: 1rem;
      margin-bottom: 0.75rem;
      align-items: flex-start;
    }}
    .step-num {{
      background: #533483;
      color: #fff;
      border-radius: 50%;
      width: 1.6rem;
      height: 1.6rem;
      flex-shrink: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.8rem;
      font-weight: bold;
    }}
    .step-text {{ padding-top: 0.15rem; line-height: 1.5; }}
    .btn {{
      display: inline-block;
      background: #533483;
      color: #fff;
      text-decoration: none;
      padding: 0.75rem 1.5rem;
      border-radius: 8px;
      font-size: 1rem;
      font-weight: 600;
      transition: background 0.2s;
    }}
    .btn:hover {{ background: #6a42a8; }}
  </style>
</head>
<body>
  <div class="card">
    <h1>Lanio</h1>
    <p class="subtitle">Stream your local media files in Stremio</p>

    <h2>Manifest URL</h2>
    <div class="url-box">{manifest_url}</div>

    <h2>How to Install</h2>
    <div class="steps">
      <div class="step">
        <div class="step-num">1</div>
        <div class="step-text">Open Stremio and click the <strong>puzzle icon</strong> (Add-ons) in the top right</div>
      </div>
      <div class="step">
        <div class="step-num">2</div>
        <div class="step-text">Click <strong>Community Add-ons</strong> at the bottom of the page</div>
      </div>
      <div class="step">
        <div class="step-num">3</div>
        <div class="step-text">Paste the manifest URL above and click <strong>Install</strong></div>
      </div>
    </div>

    <a href="{install_url}" class="btn">Install in Stremio Web</a>
  </div>
</body>
</html>"#
    )
}
