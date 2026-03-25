import React, { useState } from 'react';
import { useSecurity } from '../context/SecurityContext';
import type { AlertSeverity } from '../services/security';

const SEVERITY_COLOR: Record<AlertSeverity, string> = {
  low: '#22c55e',
  medium: '#f59e0b',
  high: '#f97316',
  critical: '#ef4444',
};

type Tab = 'overview' | 'alerts' | 'audit' | 'settings' | 'education';

export function SecurityDashboard(): JSX.Element {
  const security = useSecurity();
  const [tab, setTab] = useState<Tab>('overview');
  const [totpSecret] = useState(() => security.generateTOTPSecret());
  const [totpCode, setTotpCode] = useState('');
  const [totpResult, setTotpResult] = useState<boolean | null>(null);
  const [encInput, setEncInput] = useState('');
  const [encPass, setEncPass] = useState('');
  const [encOutput, setEncOutput] = useState('');

  const activeAlerts = security.alerts.filter((a) => !a.dismissed);
  const sessionValid = security.isSessionValid();

  async function handleBiometric() {
    await security.authenticateBiometric();
  }

  async function handleTOTP() {
    const ok = await security.verifyTOTP(totpSecret, totpCode);
    setTotpResult(ok);
    if (ok) await security.createSession(['totp']);
  }

  async function handleEncrypt() {
    const result = await security.encryptData(encInput, encPass);
    setEncOutput(result);
  }

  async function handleDecrypt() {
    try {
      const result = await security.decryptData(encOutput, encPass);
      setEncInput(result);
    } catch {
      setEncInput('Decryption failed — wrong passphrase?');
    }
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: 'overview', label: '🔐 Overview' },
    { id: 'alerts', label: `🚨 Alerts${activeAlerts.length ? ` (${activeAlerts.length})` : ''}` },
    { id: 'audit', label: '📋 Audit Log' },
    { id: 'settings', label: '⚙️ Settings' },
    { id: 'education', label: '📚 Education' },
  ];

  return (
    <div style={{ fontFamily: 'sans-serif', maxWidth: 900, margin: '0 auto', padding: 16 }}>
      <h2 style={{ marginBottom: 8 }}>Security Dashboard</h2>

      {/* Tab bar */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            style={{
              padding: '6px 14px',
              borderRadius: 6,
              border: 'none',
              cursor: 'pointer',
              background: tab === t.id ? '#6366f1' : '#e5e7eb',
              color: tab === t.id ? '#fff' : '#111',
              fontWeight: tab === t.id ? 700 : 400,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* ── Overview ── */}
      {tab === 'overview' && (
        <div style={{ display: 'grid', gap: 16, gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))' }}>
          {/* Session card */}
          <Card title="Session">
            <StatusRow label="Status" value={sessionValid ? '✅ Active' : '❌ Inactive'} />
            <StatusRow label="Locked" value={security.isLocked ? '🔒 Yes' : '🔓 No'} />
            {security.session && (
              <>
                <StatusRow label="Auth methods" value={security.session.authMethods.join(', ')} />
                <StatusRow
                  label="Expires"
                  value={new Date(security.session.expiresAt).toLocaleTimeString()}
                />
                <StatusRow label="Device" value={security.session.deviceFingerprint} />
              </>
            )}
            <div style={{ marginTop: 8, display: 'flex', gap: 8 }}>
              {security.config.biometricEnabled && (
                <Btn onClick={handleBiometric}>Biometric login</Btn>
              )}
              {sessionValid && (
                <Btn onClick={security.endSession} danger>
                  Sign out
                </Btn>
              )}
              {security.isLocked && <Btn onClick={security.unlock}>Unlock</Btn>}
            </div>
          </Card>

          {/* 2FA card */}
          <Card title="Two-Factor Auth (TOTP)">
            <p style={{ fontSize: 12, wordBreak: 'break-all', marginBottom: 8 }}>
              Secret: <code>{totpSecret}</code>
            </p>
            <p style={{ fontSize: 11, color: '#6b7280', marginBottom: 8 }}>
              Scan this secret in an authenticator app (e.g. Google Authenticator).
            </p>
            <input
              placeholder="Enter 6-digit code"
              value={totpCode}
              onChange={(e) => setTotpCode(e.target.value)}
              maxLength={6}
              style={inputStyle}
            />
            <Btn onClick={handleTOTP}>Verify &amp; login</Btn>
            {totpResult !== null && (
              <p style={{ color: totpResult ? '#22c55e' : '#ef4444', marginTop: 6 }}>
                {totpResult ? '✅ Verified' : '❌ Invalid code'}
              </p>
            )}
          </Card>

          {/* Encryption card */}
          <Card title="Secure Storage / Encryption">
            <textarea
              placeholder="Data to encrypt"
              value={encInput}
              onChange={(e) => setEncInput(e.target.value)}
              rows={3}
              style={{ ...inputStyle, resize: 'vertical' }}
            />
            <input
              placeholder="Passphrase"
              type="password"
              value={encPass}
              onChange={(e) => setEncPass(e.target.value)}
              style={inputStyle}
            />
            <div style={{ display: 'flex', gap: 8 }}>
              <Btn onClick={handleEncrypt}>Encrypt</Btn>
              <Btn onClick={handleDecrypt}>Decrypt</Btn>
            </div>
            {encOutput && (
              <textarea
                readOnly
                value={encOutput}
                rows={3}
                style={{ ...inputStyle, marginTop: 8, background: '#f9fafb', resize: 'vertical' }}
              />
            )}
          </Card>

          {/* Stats card */}
          <Card title="Security Stats">
            <StatusRow label="Failed attempts" value={String(security.failedAttempts)} />
            <StatusRow label="Active alerts" value={String(activeAlerts.length)} />
            <StatusRow label="Audit entries" value={String(security.auditLog.length)} />
            <StatusRow label="2FA required" value={security.config.require2FA ? 'Yes' : 'No'} />
            <StatusRow
              label="Session timeout"
              value={`${security.config.sessionTimeoutMs / 60_000} min`}
            />
          </Card>
        </div>
      )}

      {/* ── Alerts ── */}
      {tab === 'alerts' && (
        <div>
          {activeAlerts.length === 0 && (
            <p style={{ color: '#6b7280' }}>No active alerts.</p>
          )}
          {activeAlerts.map((alert) => (
            <div
              key={alert.id}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                padding: '10px 14px',
                marginBottom: 8,
                borderRadius: 8,
                borderLeft: `4px solid ${SEVERITY_COLOR[alert.severity]}`,
                background: '#f9fafb',
              }}
            >
              <div>
                <span
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    color: SEVERITY_COLOR[alert.severity],
                    textTransform: 'uppercase',
                    marginRight: 8,
                  }}
                >
                  {alert.severity}
                </span>
                {alert.message}
                <span style={{ fontSize: 11, color: '#9ca3af', marginLeft: 8 }}>
                  {new Date(alert.timestamp).toLocaleString()}
                </span>
              </div>
              <Btn onClick={() => security.dismissAlert(alert.id)}>Dismiss</Btn>
            </div>
          ))}
        </div>
      )}

      {/* ── Audit Log ── */}
      {tab === 'audit' && (
        <div style={{ overflowX: 'auto' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 13 }}>
            <thead>
              <tr style={{ background: '#f3f4f6' }}>
                {['Time', 'Action', 'Details', 'Severity', 'Device'].map((h) => (
                  <th key={h} style={{ padding: '8px 12px', textAlign: 'left', fontWeight: 600 }}>
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {security.auditLog.map((entry) => (
                <tr key={entry.id} style={{ borderBottom: '1px solid #e5e7eb' }}>
                  <td style={{ padding: '6px 12px', whiteSpace: 'nowrap' }}>
                    {new Date(entry.timestamp).toLocaleString()}
                  </td>
                  <td style={{ padding: '6px 12px' }}>{entry.action}</td>
                  <td style={{ padding: '6px 12px' }}>{entry.details}</td>
                  <td
                    style={{
                      padding: '6px 12px',
                      color: SEVERITY_COLOR[entry.severity],
                      fontWeight: 600,
                    }}
                  >
                    {entry.severity}
                  </td>
                  <td style={{ padding: '6px 12px', fontFamily: 'monospace', fontSize: 11 }}>
                    {entry.deviceFingerprint}
                  </td>
                </tr>
              ))}
              {security.auditLog.length === 0 && (
                <tr>
                  <td colSpan={5} style={{ padding: 16, color: '#9ca3af', textAlign: 'center' }}>
                    No audit entries yet.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {/* ── Settings ── */}
      {tab === 'settings' && (
        <div style={{ display: 'grid', gap: 16, maxWidth: 480 }}>
          <Card title="Session">
            <label style={labelStyle}>
              Timeout (minutes)
              <input
                type="number"
                min={1}
                max={1440}
                value={security.config.sessionTimeoutMs / 60_000}
                onChange={(e) =>
                  security.updateConfig({ sessionTimeoutMs: Number(e.target.value) * 60_000 })
                }
                style={inputStyle}
              />
            </label>
            <label style={labelStyle}>
              Max failed attempts
              <input
                type="number"
                min={1}
                max={20}
                value={security.config.maxFailedAttempts}
                onChange={(e) =>
                  security.updateConfig({ maxFailedAttempts: Number(e.target.value) })
                }
                style={inputStyle}
              />
            </label>
          </Card>
          <Card title="Authentication">
            <ToggleRow
              label="Require 2FA"
              checked={security.config.require2FA}
              onChange={(v) => security.updateConfig({ require2FA: v })}
            />
            <ToggleRow
              label="Biometric enabled"
              checked={security.config.biometricEnabled}
              onChange={(v) => security.updateConfig({ biometricEnabled: v })}
              disabled={!security.config.biometricEnabled}
            />
          </Card>
        </div>
      )}

      {/* ── Education ── */}
      {tab === 'education' && (
        <div style={{ display: 'grid', gap: 12 }}>
          {SECURITY_TIPS.map((tip) => (
            <div
              key={tip.title}
              style={{ padding: 14, borderRadius: 8, background: '#f0fdf4', border: '1px solid #bbf7d0' }}
            >
              <strong>{tip.icon} {tip.title}</strong>
              <p style={{ margin: '4px 0 0', fontSize: 13, color: '#374151' }}>{tip.body}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Small helpers ────────────────────────────────────────────────────────────

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ border: '1px solid #e5e7eb', borderRadius: 10, padding: 16 }}>
      <h3 style={{ margin: '0 0 12px', fontSize: 15 }}>{title}</h3>
      {children}
    </div>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6, fontSize: 13 }}>
      <span style={{ color: '#6b7280' }}>{label}</span>
      <span style={{ fontWeight: 500 }}>{value}</span>
    </div>
  );
}

function Btn({
  onClick,
  children,
  danger,
}: {
  onClick: () => void;
  children: React.ReactNode;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: '5px 12px',
        borderRadius: 6,
        border: 'none',
        cursor: 'pointer',
        background: danger ? '#fee2e2' : '#ede9fe',
        color: danger ? '#b91c1c' : '#4f46e5',
        fontSize: 13,
        fontWeight: 500,
      }}
    >
      {children}
    </button>
  );
}

function ToggleRow({
  label,
  checked,
  onChange,
  disabled,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label
      style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        marginBottom: 10,
        fontSize: 13,
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {label}
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '6px 10px',
  borderRadius: 6,
  border: '1px solid #d1d5db',
  marginBottom: 8,
  fontSize: 13,
  boxSizing: 'border-box',
};

const labelStyle: React.CSSProperties = {
  display: 'block',
  fontSize: 13,
  marginBottom: 10,
  color: '#374151',
};

const SECURITY_TIPS = [
  {
    icon: '🔑',
    title: 'Use strong, unique passwords',
    body: 'Never reuse passwords across services. Use a password manager to generate and store complex passwords.',
  },
  {
    icon: '📱',
    title: 'Enable two-factor authentication',
    body: 'TOTP-based 2FA adds a second layer of protection. Even if your password is compromised, attackers cannot access your account without the code.',
  },
  {
    icon: '🖐️',
    title: 'Biometric authentication',
    body: 'Where available, biometric login (fingerprint / face) is both convenient and secure — your biometric data never leaves your device.',
  },
  {
    icon: '⏱️',
    title: 'Session timeouts protect you',
    body: 'Short session timeouts reduce the risk of unauthorised access on shared or unattended devices.',
  },
  {
    icon: '🔒',
    title: 'Encrypt sensitive data at rest',
    body: 'Use the built-in AES-GCM encryption to protect private keys and sensitive information stored locally.',
  },
  {
    icon: '🚨',
    title: 'Monitor security alerts',
    body: 'Review alerts regularly. Multiple failed login attempts or unusual activity may indicate an attack.',
  },
  {
    icon: '📋',
    title: 'Audit logs for compliance',
    body: 'All security-relevant actions are logged with timestamps and device fingerprints to support compliance and forensic investigation.',
  },
];
