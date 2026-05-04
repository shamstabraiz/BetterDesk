/* policies.js — Organization Policy Management */
(function () {
  'use strict';

  let currentOrgId = null;
  let policyData = {};

  /* ── Bootstrap ─────────────────────────────────────── */
  document.addEventListener('DOMContentLoaded', init);

  function init() {
    loadOrganizations();
    document.getElementById('policy-org-select').addEventListener('change', onOrgChange);
  }

  /* ── Organizations ─────────────────────────────────── */
  async function loadOrganizations() {
    try {
      const resp = await fetch('/api/panel/org', { credentials: 'same-origin' });
      if (!resp.ok) return;
      const data = await resp.json();
      const orgs = data.organizations || data.data || data || [];
      const sel = document.getElementById('policy-org-select');
      (Array.isArray(orgs) ? orgs : []).forEach(o => {
        const opt = document.createElement('option');
        opt.value = o.id;
        opt.textContent = o.name || o.id;
        sel.appendChild(opt);
      });
    } catch (e) { console.warn('Failed to load organizations:', e); }
  }

  async function onOrgChange() {
    const sel = document.getElementById('policy-org-select');
    currentOrgId = sel.value;
    if (!currentOrgId) {
      document.getElementById('policy-sections').style.display = 'none';
      return;
    }
    document.getElementById('policy-sections').style.display = '';
    await fetchPolicies();
  }

  /* ── Fetch policies ─────────────────────────────────── */
  async function fetchPolicies() {
    if (!currentOrgId) return;
    try {
      const resp = await fetch(`/api/panel/policies/${encodeURIComponent(currentOrgId)}`, { credentials: 'same-origin' });
      if (!resp.ok) throw new Error(resp.statusText);
      const raw = await resp.json();
      policyData = raw.data || raw || {};
      populateForms();
      loadAuditLog();
    } catch (e) {
      console.warn('Failed to fetch policies:', e);
      if (typeof Notifications !== 'undefined') Notifications.error(_('policies.load_failed') || 'Failed to load policies');
    }
  }

  /* ── Populate ────────────────────────────────────────── */
  function populateForms() {
    const c = policyData.connection || {};
    setVal('pol-connection-mode', c.mode || 'attended');
    setVal('pol-max-session', c.max_session_duration || 480);
    setVal('pol-idle-lock', c.idle_lock_timeout || 15);
    setVal('pol-wallpaper', c.wallpaper || 'show');
    setBadge('conn-status', c);

    const f = policyData.features || {};
    setChecked('pol-file-transfer', f.allow_file_transfer !== false);
    setChecked('pol-clipboard', f.allow_clipboard !== false);
    setChecked('pol-audio', !!f.allow_audio);
    setChecked('pol-recording', f.allow_recording !== false);
    setBadge('feat-status', f);

    const s = policyData.security || {};
    setChecked('pol-require-2fa', s.require_2fa !== false);
    setVal('pol-pass-min', s.password_min_length || 12);
    setChecked('pol-pass-special', s.password_require_special !== false);
    setVal('pol-pass-age', s.password_max_age_days || 90);
    setVal('pol-allowed-ops', (s.allowed_operators || []).join(', '));
    setBadge('sec-status', s);

    const n = policyData.network || {};
    setChecked('pol-block-p2p', !!n.block_direct_p2p);
    setVal('pol-relay-servers', (n.allowed_relay_servers || []).join('\n'));
    setVal('pol-ip-allowlist', (n.ip_allowlist || []).join('\n'));
    setBadge('net-status', n);

    const u = policyData.update || {};
    setChecked('pol-auto-update', u.auto_update !== false);
    setVal('pol-update-channel', u.channel || 'stable');
    setBadge('upd-status', u);
  }

  function setBadge(id, obj) {
    const el = document.getElementById(id);
    if (!el) return;
    const hasData = Object.keys(obj).length > 0;
    el.textContent = hasData ? (typeof _ === 'function' ? _('policies.configured') : 'Configured') : '—';
    el.className = 'policy-status-badge' + (hasData ? ' configured' : '');
  }

  /* ── Save section ───────────────────────────────────── */
  async function saveSection(section) {
    if (!currentOrgId) return;

    let body = {};
    switch (section) {
      case 'connection':
        body = {
          mode: getVal('pol-connection-mode'),
          max_session_duration: parseInt(getVal('pol-max-session')) || 480,
          idle_lock_timeout: parseInt(getVal('pol-idle-lock')) || 15,
          wallpaper: getVal('pol-wallpaper')
        };
        break;
      case 'features':
        body = {
          allow_file_transfer: isChecked('pol-file-transfer'),
          allow_clipboard: isChecked('pol-clipboard'),
          allow_audio: isChecked('pol-audio'),
          allow_recording: isChecked('pol-recording')
        };
        break;
      case 'security':
        body = {
          require_2fa: isChecked('pol-require-2fa'),
          password_min_length: parseInt(getVal('pol-pass-min')) || 12,
          password_require_special: isChecked('pol-pass-special'),
          password_max_age_days: parseInt(getVal('pol-pass-age')) || 0,
          allowed_operators: getVal('pol-allowed-ops').split(',').map(s => s.trim()).filter(Boolean)
        };
        break;
      case 'network':
        body = {
          block_direct_p2p: isChecked('pol-block-p2p'),
          allowed_relay_servers: getVal('pol-relay-servers').split('\n').map(s => s.trim()).filter(Boolean),
          ip_allowlist: getVal('pol-ip-allowlist').split('\n').map(s => s.trim()).filter(Boolean)
        };
        break;
      case 'update':
        body = {
          auto_update: isChecked('pol-auto-update'),
          channel: getVal('pol-update-channel')
        };
        break;
      default:
        return;
    }

    try {
      const csrfToken = window.Yomie?.csrfToken || '';
      const resp = await fetch(`/api/panel/policies/${encodeURIComponent(currentOrgId)}/${section}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
          ...(csrfToken ? { 'x-csrf-token': csrfToken } : {})
        },
        credentials: 'same-origin',
        body: JSON.stringify(body)
      });
      if (!resp.ok) throw new Error(resp.statusText);
      if (typeof Notifications !== 'undefined') Notifications.success(_('policies.save_success') || `${section} policy updated`);
      await fetchPolicies();
    } catch (e) {
      console.error('Save failed:', e);
      if (typeof Notifications !== 'undefined') Notifications.error(_('policies.save_failed') || 'Failed to save policy');
    }
  }

  /* ── Audit log ──────────────────────────────────────── */
  async function loadAuditLog() {
    if (!currentOrgId) return;
    const container = document.getElementById('policy-audit-list');
    try {
      const resp = await fetch(`/api/panel/policies/${encodeURIComponent(currentOrgId)}/audit`, { credentials: 'same-origin' });
      if (!resp.ok) throw new Error(resp.statusText);
      const raw = await resp.json();
      const entries = raw.data || raw.entries || raw || [];
      if (!Array.isArray(entries) || entries.length === 0) {
        container.innerHTML = `<p class="text-muted">${typeof _ === 'function' ? _('policies.no_audit') : 'No policy changes recorded.'}</p>`;
        return;
      }
      container.innerHTML = entries.map(e => `
        <div class="audit-entry">
          <span class="audit-time">${new Date(e.timestamp || e.created_at).toLocaleString()}</span>
          <span class="audit-actor">${escapeHtml(e.actor || e.user || '?')}</span>
          <span class="audit-section badge">${escapeHtml(e.section || e.category || '')}</span>
          <span class="audit-detail">${escapeHtml(e.detail || e.description || '')}</span>
        </div>
      `).join('');
    } catch (e) {
      container.innerHTML = '<p class="text-muted">Failed to load audit log.</p>';
    }
  }

  /* ── Helpers ─────────────────────────────────────────── */
  function getVal(id) { const el = document.getElementById(id); return el ? el.value : ''; }
  function setVal(id, v) { const el = document.getElementById(id); if (el) el.value = v; }
  function isChecked(id) { const el = document.getElementById(id); return el ? el.checked : false; }
  function setChecked(id, v) { const el = document.getElementById(id); if (el) el.checked = !!v; }
  function escapeHtml(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

  /* ── Public API ──────────────────────────────────────── */
  window.Policies = { saveSection };
})();
