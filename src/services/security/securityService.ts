import type {
  SecuritySession,
  AuditLogEntry,
  SecurityAlert,
  SecurityConfig,
  SecurityState,
  AuthMethod,
  AlertSeverity,
} from './types';

const DEFAULT_CONFIG: SecurityConfig = {
  sessionTimeoutMs: 30 * 60 * 1000, // 30 minutes
  maxFailedAttempts: 5,
  require2FA: false,
  biometricEnabled: false,
};

const STORAGE_KEY = 'soroban_security_state';

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

/** Stable device fingerprint derived from browser signals */
async function getDeviceFingerprint(): Promise<string> {
  const signals = [
    navigator.userAgent,
    navigator.language,
    screen.width,
    screen.height,
    screen.colorDepth,
    Intl.DateTimeFormat().resolvedOptions().timeZone,
    navigator.hardwareConcurrency ?? 0,
  ].join('|');

  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(signals));
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
    .slice(0, 16);
}

/** Encrypt a string using AES-GCM with a key derived from a passphrase */
async function encryptData(data: string, passphrase: string): Promise<string> {
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(passphrase),
    'PBKDF2',
    false,
    ['deriveKey'],
  );
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const key = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100_000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt'],
  );
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ciphertext = await crypto.subtle.encrypt(
    { name: 'AES-GCM', iv },
    key,
    new TextEncoder().encode(data),
  );
  // Pack: salt(16) + iv(12) + ciphertext
  const packed = new Uint8Array(16 + 12 + ciphertext.byteLength);
  packed.set(salt, 0);
  packed.set(iv, 16);
  packed.set(new Uint8Array(ciphertext), 28);
  return btoa(String.fromCharCode(...packed));
}

/** Decrypt a string encrypted by encryptData */
async function decryptData(encoded: string, passphrase: string): Promise<string> {
  const packed = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
  const salt = packed.slice(0, 16);
  const iv = packed.slice(16, 28);
  const ciphertext = packed.slice(28);
  const keyMaterial = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(passphrase),
    'PBKDF2',
    false,
    ['deriveKey'],
  );
  const key = await crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt, iterations: 100_000, hash: 'SHA-256' },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['decrypt'],
  );
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, key, ciphertext);
  return new TextDecoder().decode(plain);
}

class SecurityService {
  private state: SecurityState = {
    session: null,
    alerts: [],
    auditLog: [],
    failedAttempts: 0,
    isLocked: false,
    config: DEFAULT_CONFIG,
  };

  private listeners: Array<(state: SecurityState) => void> = [];
  private sessionTimer: ReturnType<typeof setTimeout> | null = null;

  // ── Initialisation ──────────────────────────────────────────────────────────

  async init(): Promise<void> {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      try {
        const saved = JSON.parse(raw) as Partial<SecurityState>;
        this.state = {
          ...this.state,
          ...saved,
          config: { ...DEFAULT_CONFIG, ...(saved.config ?? {}) },
        };
      } catch {
        // corrupt storage — start fresh
      }
    }
    this.state.config.biometricEnabled = await this.isBiometricAvailable();
    this.scheduleSessionExpiry();
    this.notify();
  }

  // ── Biometric ───────────────────────────────────────────────────────────────

  async isBiometricAvailable(): Promise<boolean> {
    return (
      typeof window !== 'undefined' &&
      'PublicKeyCredential' in window &&
      (await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable().catch(() => false))
    );
  }

  async authenticateBiometric(): Promise<boolean> {
    if (!(await this.isBiometricAvailable())) return false;
    try {
      const challenge = crypto.getRandomValues(new Uint8Array(32));
      await navigator.credentials.get({
        publicKey: {
          challenge,
          timeout: 60_000,
          userVerification: 'required',
          rpId: window.location.hostname,
          allowCredentials: [],
        },
      });
      await this.createSession(['biometric']);
      this.log('biometric_auth_success', 'Biometric authentication succeeded', 'low');
      return true;
    } catch {
      this.recordFailedAttempt('biometric_auth_failed', 'Biometric authentication failed');
      return false;
    }
  }

  // ── TOTP (2FA) ──────────────────────────────────────────────────────────────

  /** Generate a TOTP secret (base32-encoded random bytes) */
  generateTOTPSecret(): string {
    const bytes = crypto.getRandomValues(new Uint8Array(20));
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
    let result = '';
    for (let i = 0; i < bytes.length; i++) {
      result += alphabet[bytes[i] % 32];
    }
    return result;
  }

  /** Verify a 6-digit TOTP code against a secret (RFC 6238, SHA-1, 30s window) */
  async verifyTOTP(secret: string, code: string): Promise<boolean> {
    const counter = Math.floor(Date.now() / 1000 / 30);
    for (const offset of [-1, 0, 1]) {
      if ((await this.computeHOTP(secret, counter + offset)) === code) {
        this.log('totp_verified', '2FA code verified', 'low');
        return true;
      }
    }
    this.recordFailedAttempt('totp_failed', 'Invalid 2FA code');
    return false;
  }

  private async computeHOTP(secret: string, counter: number): Promise<string> {
    const keyBytes = this.base32Decode(secret);
    const counterBytes = new Uint8Array(8);
    let c = counter;
    for (let i = 7; i >= 0; i--) {
      counterBytes[i] = c & 0xff;
      c >>= 8;
    }
    const key = await crypto.subtle.importKey(
      'raw',
      keyBytes,
      { name: 'HMAC', hash: 'SHA-1' },
      false,
      ['sign'],
    );
    const sig = new Uint8Array(await crypto.subtle.sign('HMAC', key, counterBytes));
    const offset = sig[19] & 0xf;
    const otp =
      (((sig[offset] & 0x7f) << 24) |
        ((sig[offset + 1] & 0xff) << 16) |
        ((sig[offset + 2] & 0xff) << 8) |
        (sig[offset + 3] & 0xff)) %
      1_000_000;
    return otp.toString().padStart(6, '0');
  }

  private base32Decode(input: string): Uint8Array {
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
    const clean = input.toUpperCase().replace(/=+$/, '');
    let bits = 0;
    let value = 0;
    const output: number[] = [];
    for (const char of clean) {
      value = (value << 5) | alphabet.indexOf(char);
      bits += 5;
      if (bits >= 8) {
        output.push((value >>> (bits - 8)) & 0xff);
        bits -= 8;
      }
    }
    return new Uint8Array(output);
  }

  // ── Session management ───────────────────────────────────────────────────────

  async createSession(methods: AuthMethod[]): Promise<SecuritySession> {
    const fingerprint = await getDeviceFingerprint();
    const now = Date.now();
    const session: SecuritySession = {
      id: generateId(),
      deviceFingerprint: fingerprint,
      createdAt: now,
      lastActiveAt: now,
      expiresAt: now + this.state.config.sessionTimeoutMs,
      authMethods: methods,
    };
    this.state.session = session;
    this.state.failedAttempts = 0;
    this.state.isLocked = false;
    this.scheduleSessionExpiry();
    this.log('session_created', `Session created via ${methods.join(', ')}`, 'low');
    this.persist();
    this.notify();
    return session;
  }

  touchSession(): void {
    if (!this.state.session) return;
    const now = Date.now();
    this.state.session.lastActiveAt = now;
    this.state.session.expiresAt = now + this.state.config.sessionTimeoutMs;
    this.scheduleSessionExpiry();
    this.persist();
  }

  endSession(): void {
    if (this.state.session) {
      this.log('session_ended', 'Session terminated', 'low');
    }
    this.state.session = null;
    if (this.sessionTimer) clearTimeout(this.sessionTimer);
    this.persist();
    this.notify();
  }

  isSessionValid(): boolean {
    const s = this.state.session;
    return !!s && Date.now() < s.expiresAt && !this.state.isLocked;
  }

  private scheduleSessionExpiry(): void {
    if (this.sessionTimer) clearTimeout(this.sessionTimer);
    if (!this.state.session) return;
    const remaining = this.state.session.expiresAt - Date.now();
    if (remaining <= 0) {
      this.expireSession();
      return;
    }
    this.sessionTimer = setTimeout(() => this.expireSession(), remaining);
  }

  private expireSession(): void {
    this.addAlert('Session expired due to inactivity', 'medium');
    this.log('session_expired', 'Session expired', 'medium');
    this.state.session = null;
    this.persist();
    this.notify();
  }

  // ── Suspicious activity ──────────────────────────────────────────────────────

  private recordFailedAttempt(action: string, details: string): void {
    this.state.failedAttempts += 1;
    this.log(action, details, 'medium');
    if (this.state.failedAttempts >= this.state.config.maxFailedAttempts) {
      this.state.isLocked = true;
      this.addAlert(
        `Account locked after ${this.state.failedAttempts} failed attempts`,
        'critical',
      );
      this.log('account_locked', 'Account locked due to repeated failures', 'critical');
    }
    this.persist();
    this.notify();
  }

  detectSuspiciousActivity(action: string): void {
    const recentCritical = this.state.auditLog.filter(
      (e) => e.severity === 'critical' && Date.now() - e.timestamp < 60_000,
    );
    if (recentCritical.length >= 3) {
      this.addAlert('Multiple critical events detected in the last minute', 'critical');
    }
    this.log(action, 'Suspicious activity detected', 'high');
    this.persist();
    this.notify();
  }

  // ── Encryption helpers (public API) ─────────────────────────────────────────

  async encryptSensitiveData(data: string, passphrase: string): Promise<string> {
    return encryptData(data, passphrase);
  }

  async decryptSensitiveData(encoded: string, passphrase: string): Promise<string> {
    return decryptData(encoded, passphrase);
  }

  // ── Alerts ───────────────────────────────────────────────────────────────────

  addAlert(message: string, severity: AlertSeverity): void {
    this.state.alerts.unshift({
      id: generateId(),
      timestamp: Date.now(),
      message,
      severity,
      dismissed: false,
    });
    // Keep last 50 alerts
    this.state.alerts = this.state.alerts.slice(0, 50);
    this.persist();
    this.notify();
  }

  dismissAlert(id: string): void {
    const alert = this.state.alerts.find((a) => a.id === id);
    if (alert) {
      alert.dismissed = true;
      this.persist();
      this.notify();
    }
  }

  // ── Audit log ────────────────────────────────────────────────────────────────

  private async log(action: string, details: string, severity: AlertSeverity): Promise<void> {
    const fingerprint = this.state.session?.deviceFingerprint ?? await getDeviceFingerprint();
    this.state.auditLog.unshift({
      id: generateId(),
      timestamp: Date.now(),
      action,
      details,
      severity,
      deviceFingerprint: fingerprint,
    });
    // Keep last 200 entries
    this.state.auditLog = this.state.auditLog.slice(0, 200);
  }

  // ── Config ───────────────────────────────────────────────────────────────────

  updateConfig(patch: Partial<SecurityConfig>): void {
    this.state.config = { ...this.state.config, ...patch };
    this.persist();
    this.notify();
  }

  unlock(): void {
    this.state.isLocked = false;
    this.state.failedAttempts = 0;
    this.log('account_unlocked', 'Account manually unlocked', 'medium');
    this.persist();
    this.notify();
  }

  // ── State access ─────────────────────────────────────────────────────────────

  getState(): SecurityState {
    return { ...this.state };
  }

  subscribe(listener: (state: SecurityState) => void): () => void {
    this.listeners.push(listener);
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  }

  private notify(): void {
    const snapshot = this.getState();
    this.listeners.forEach((l) => l(snapshot));
  }

  private persist(): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.state));
    } catch {
      // storage full — ignore
    }
  }
}

export const securityService = new SecurityService();
export { encryptData, decryptData };
