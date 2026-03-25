export type AuthMethod = 'biometric' | 'totp' | 'password';
export type AlertSeverity = 'low' | 'medium' | 'high' | 'critical';

export interface SecuritySession {
  id: string;
  deviceFingerprint: string;
  createdAt: number;
  lastActiveAt: number;
  expiresAt: number;
  authMethods: AuthMethod[];
}

export interface AuditLogEntry {
  id: string;
  timestamp: number;
  action: string;
  details: string;
  severity: AlertSeverity;
  deviceFingerprint: string;
}

export interface SecurityAlert {
  id: string;
  timestamp: number;
  message: string;
  severity: AlertSeverity;
  dismissed: boolean;
}

export interface SecurityConfig {
  sessionTimeoutMs: number;
  maxFailedAttempts: number;
  require2FA: boolean;
  biometricEnabled: boolean;
}

export interface SecurityState {
  session: SecuritySession | null;
  alerts: SecurityAlert[];
  auditLog: AuditLogEntry[];
  failedAttempts: number;
  isLocked: boolean;
  config: SecurityConfig;
}
