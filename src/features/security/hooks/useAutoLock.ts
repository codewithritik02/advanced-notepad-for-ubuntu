import { useEffect, useRef, useState, useCallback } from "react";
import { settingsStorage } from "../../../services/storage/settings";
import { logger } from "../../../utils/logger";

export interface AutoLockSettings {
  autoLockEnabled: boolean;
  inactivityTimeoutMinutes: number;
  lockOnSleep: boolean;
}

export const DEFAULT_AUTO_LOCK_SETTINGS: AutoLockSettings = {
  autoLockEnabled: true,
  inactivityTimeoutMinutes: 15,
  lockOnSleep: true,
};

export const AUTO_LOCK_SETTINGS_KEY = "auto_lock_settings";

export const ALLOWED_AUTO_LOCK_TIMEOUTS = [1, 5, 15, 30, 60] as const;

/**
 * Retrieves persisted auto-lock settings from storage, with safe fallback defaults.
 */
export async function getAutoLockSettings(): Promise<AutoLockSettings> {
  try {
    const raw = await settingsStorage.get(AUTO_LOCK_SETTINGS_KEY);
    if (!raw) return DEFAULT_AUTO_LOCK_SETTINGS;
    const parsed = JSON.parse(raw);
    return {
      autoLockEnabled: typeof parsed.autoLockEnabled === "boolean" ? parsed.autoLockEnabled : true,
      inactivityTimeoutMinutes: ALLOWED_AUTO_LOCK_TIMEOUTS.includes(parsed.inactivityTimeoutMinutes)
        ? parsed.inactivityTimeoutMinutes
        : 15,
      lockOnSleep: typeof parsed.lockOnSleep === "boolean" ? parsed.lockOnSleep : true,
    };
  } catch {
    return DEFAULT_AUTO_LOCK_SETTINGS;
  }
}

/**
 * Persists auto-lock configuration to SQLite settings table.
 */
export async function saveAutoLockSettings(settings: AutoLockSettings): Promise<void> {
  await settingsStorage.set(AUTO_LOCK_SETTINGS_KEY, JSON.stringify(settings));
}

export interface UseAutoLockOptions {
  isEncryptionEnabled: boolean;
  isAppLocked: boolean;
  onLock: () => Promise<void> | void;
}

export interface UseAutoLockReturn {
  settings: AutoLockSettings;
  updateSettings: (newSettings: Partial<AutoLockSettings>) => Promise<void>;
  resetInactivityTimer: () => void;
}

/**
 * Centralized Automatic Locking and Inactivity Policy hook (Phase 12, Task 14).
 *
 * Capabilities:
 * - Detects user inactivity across desktop interaction events (mouse, keyboard, touch, scroll).
 * - Throttles high-frequency mouse movements to prevent performance degradation.
 * - Detects operating system sleep / resume via timer drift / leap comparison.
 * - Enforces immediate lock on window visibility return if timeout elapsed while hidden.
 * - Prevents duplicate lock invocations and concurrent race conditions with lock guards.
 * - Guarantees "Save-Before-Lock" execution through the onLock callback.
 */
export function useAutoLock({
  isEncryptionEnabled,
  isAppLocked,
  onLock,
}: UseAutoLockOptions): UseAutoLockReturn {
  const [settings, setSettings] = useState<AutoLockSettings>(DEFAULT_AUTO_LOCK_SETTINGS);

  const lastActivityRef = useRef<number>(Date.now());
  const lastTickRef = useRef<number>(Date.now());
  const lastMouseMoveRecordRef = useRef<number>(0);
  const isLockingRef = useRef<boolean>(false);

  // Load persisted settings on mount
  useEffect(() => {
    let isMounted = true;
    getAutoLockSettings().then((loaded) => {
      if (isMounted) {
        setSettings(loaded);
      }
    });
    return () => {
      isMounted = false;
    };
  }, []);

  const resetInactivityTimer = useCallback(() => {
    lastActivityRef.current = Date.now();
  }, []);

  const updateSettings = useCallback(
    async (partial: Partial<AutoLockSettings>) => {
      setSettings((prev) => {
        const next: AutoLockSettings = {
          ...prev,
          ...partial,
        };
        saveAutoLockSettings(next).catch((err) => {
          logger.warn("Failed to persist auto-lock settings:", { error: String(err) });
        });
        return next;
      });
      lastActivityRef.current = Date.now();
    },
    []
  );

  const triggerLock = useCallback(
    async (reason: string) => {
      if (!isEncryptionEnabled || isAppLocked || !settings.autoLockEnabled || isLockingRef.current) {
        return;
      }

      isLockingRef.current = true;
      try {
        logger.info(`Auto-lock triggered: ${reason}`);
        await onLock();
      } catch (err) {
        logger.error("Auto-lock invocation failed:", err);
      } finally {
        isLockingRef.current = false;
      }
    },
    [isEncryptionEnabled, isAppLocked, settings.autoLockEnabled, onLock]
  );

  useEffect(() => {
    if (!isEncryptionEnabled || isAppLocked || !settings.autoLockEnabled) {
      return;
    }

    // Reset activity timer when unlocked or policy enabled
    lastActivityRef.current = Date.now();
    lastTickRef.current = Date.now();

    // Meaningful interaction listener
    const handleUserActivity = (event?: Event) => {
      const now = Date.now();
      if (event?.type === "mousemove") {
        // Throttle mouse moves to at most once per second
        if (now - lastMouseMoveRecordRef.current < 1000) {
          return;
        }
        lastMouseMoveRecordRef.current = now;
      }
      lastActivityRef.current = now;
    };

    window.addEventListener("mousemove", handleUserActivity, { passive: true });
    window.addEventListener("mousedown", handleUserActivity, { passive: true });
    window.addEventListener("keydown", handleUserActivity, { passive: true });
    window.addEventListener("touchstart", handleUserActivity, { passive: true });
    window.addEventListener("scroll", handleUserActivity, { passive: true });
    window.addEventListener("wheel", handleUserActivity, { passive: true });

    // Periodic heartbeat check (every 2000ms)
    const intervalId = window.setInterval(() => {
      if (isLockingRef.current) return;

      const now = Date.now();
      const timeSinceLastTick = now - lastTickRef.current;
      lastTickRef.current = now;

      // Sleep / Resume detection via timer drift:
      // If the 2-second interval took > 10 seconds, the system was suspended or in sleep mode.
      if (settings.lockOnSleep && timeSinceLastTick > 10000) {
        triggerLock("System resumed from sleep or suspended state");
        return;
      }

      // Inactivity timeout detection
      const timeoutMs = settings.inactivityTimeoutMinutes * 60 * 1000;
      const idleTime = now - lastActivityRef.current;
      if (idleTime >= timeoutMs) {
        triggerLock(`Inactivity timeout reached (${settings.inactivityTimeoutMinutes}m)`);
      }
    }, 2000);

    // Visibility change listener (e.g., returning from minimized or switched desktop)
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        const now = Date.now();
        const timeoutMs = settings.inactivityTimeoutMinutes * 60 * 1000;
        const idleTime = now - lastActivityRef.current;
        const timeSinceLastTick = now - lastTickRef.current;

        if (settings.lockOnSleep && timeSinceLastTick > 10000) {
          triggerLock("System resumed from standby while hidden");
          return;
        }

        if (idleTime >= timeoutMs) {
          triggerLock("Inactivity timeout elapsed while window was in background");
        }
      }
    };

    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      window.removeEventListener("mousemove", handleUserActivity);
      window.removeEventListener("mousedown", handleUserActivity);
      window.removeEventListener("keydown", handleUserActivity);
      window.removeEventListener("touchstart", handleUserActivity);
      window.removeEventListener("scroll", handleUserActivity);
      window.removeEventListener("wheel", handleUserActivity);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      window.clearInterval(intervalId);
    };
  }, [
    isEncryptionEnabled,
    isAppLocked,
    settings.autoLockEnabled,
    settings.inactivityTimeoutMinutes,
    settings.lockOnSleep,
    triggerLock,
  ]);

  return {
    settings,
    updateSettings,
    resetInactivityTimer,
  };
}
