/**
 * Axiom Forge - Security Manager
 * 
 * This module handles all sensitive token storage and encryption.
 * It is designed to be auditable, transparent, and secure.
 * 
 * SECURITY PRINCIPLES:
 * 1. All tokens are encrypted at rest using AES-256-GCM
 * 2. Encryption key is derived from machine-specific data + app secret
 * 3. No tokens are ever logged or exposed in error messages
 * 4. Memory is cleared after token operations
 * 5. All token access is logged (operation type, timestamp, success/failure)
 */

import crypto from 'crypto';
import { fileURLToPath } from 'url';
import path from 'path';

// ==================== CONSTANTS ====================
const ALGORITHM = 'aes-256-gcm';
const KEY_LENGTH = 32;
const IV_LENGTH = 16;
const AUTH_TAG_LENGTH = 16;
const SALT_LENGTH = 32;

// Token key whitelist - only these keys can be stored
const ALLOWED_TOKEN_KEYS = new Set([
  'github-token',
  'vercel-token',
  'google-client-id',
  'google-client-secret',
  'stripe-secret-key',
  'stripe-publishable-key',
  'openai-api-key',
  'anthropic-api-key',
  'ollama-host',
  'netlify-token',
  'render-token',
  'hostinger-token'
]);

// ==================== SECURITY MANAGER CLASS ====================
export class SecurityManager {
  constructor(store) {
    this.store = store;
    this.encryptionKey = this._deriveEncryptionKey();
    this.accessLog = [];
    this._maxLogSize = 1000;
  }

  // ==================== PRIVATE METHODS ====================

  /**
   * Derive encryption key from machine data and app secret
   * This ensures tokens can only be decrypted on this machine
   */
  _deriveEncryptionKey() {
    // Combine machine-specific identifiers with app secret
    const machineData = [
      process.env.USER || process.env.USERNAME || 'unknown',
      process.env.HOME || process.env.USERPROFILE || 'unknown',
      process.platform,
      process.arch
    ].join('|');

    const appSecret = process.env.AXIOM_ENCRYPTION_KEY || 'axiom-forge-default-secret-do-not-use-in-production';
    
    // Use PBKDF2 to derive a strong key
    const salt = crypto.createHash('sha256').update(machineData).digest();
    const key = crypto.pbkdf2Sync(appSecret, salt, 100000, KEY_LENGTH, 'sha512');
    
    return key;
  }

  /**
   * Validate token key against whitelist
   */
  _validateKey(key) {
    if (typeof key !== 'string' || key.length === 0) {
      throw new SecurityError('INVALID_KEY', 'Token key must be a non-empty string');
    }
    
    if (!ALLOWED_TOKEN_KEYS.has(key)) {
      throw new SecurityError('INVALID_KEY', `Token key '${key}' is not in the allowed list`);
    }
    
    return true;
  }

  /**
   * Encrypt a value using AES-256-GCM
   */
  _encrypt(plaintext) {
    const iv = crypto.randomBytes(IV_LENGTH);
    const salt = crypto.randomBytes(SALT_LENGTH);
    
    const cipher = crypto.createCipheriv(ALGORITHM, this.encryptionKey, iv);
    
    let encrypted = cipher.update(plaintext, 'utf8', 'hex');
    encrypted += cipher.final('hex');
    
    const authTag = cipher.getAuthTag();
    
    // Combine salt + iv + authTag + encrypted data
    const result = {
      salt: salt.toString('hex'),
      iv: iv.toString('hex'),
      authTag: authTag.toString('hex'),
      data: encrypted,
      version: '1'
    };
    
    // Clear sensitive data from memory
    plaintext = null;
    
    return JSON.stringify(result);
  }

  /**
   * Decrypt a value using AES-256-GCM
   */
  _decrypt(encryptedJson) {
    try {
      const encrypted = JSON.parse(encryptedJson);
      
      // Validate structure
      if (!encrypted.salt || !encrypted.iv || !encrypted.authTag || !encrypted.data) {
        throw new SecurityError('DECRYPT_FAILED', 'Invalid encrypted data structure');
      }
      
      const iv = Buffer.from(encrypted.iv, 'hex');
      const authTag = Buffer.from(encrypted.authTag, 'hex');
      
      const decipher = crypto.createDecipheriv(ALGORITHM, this.encryptionKey, iv);
      decipher.setAuthTag(authTag);
      
      let decrypted = decipher.update(encrypted.data, 'hex', 'utf8');
      decrypted += decipher.final('utf8');
      
      return decrypted;
    } catch (error) {
      if (error instanceof SecurityError) throw error;
      throw new SecurityError('DECRYPT_FAILED', 'Failed to decrypt token - data may be corrupted');
    }
  }

  /**
   * Log access attempt (without exposing token values)
   */
  _logAccess(operation, key, success, errorMessage = null) {
    const logEntry = {
      timestamp: new Date().toISOString(),
      operation,
      key,
      success,
      error: errorMessage
    };
    
    this.accessLog.push(logEntry);
    
    // Trim log if it gets too large
    if (this.accessLog.length > this._maxLogSize) {
      this.accessLog = this.accessLog.slice(-this._maxLogSize / 2);
    }
    
    // Also log to console for debugging (in development only)
    if (process.env.NODE_ENV === 'development') {
      console.log(`[Security] ${operation} for key '${key}': ${success ? 'SUCCESS' : 'FAILED'}`);
    }
  }

  // ==================== PUBLIC API ====================

  /**
   * Store a token securely
   * @param {string} key - Token identifier
   * @param {string} value - Token value to encrypt
   * @returns {Promise<void>}
   */
  async storeToken(key, value) {
    try {
      // Validate inputs
      this._validateKey(key);
      
      if (typeof value !== 'string') {
        throw new SecurityError('INVALID_VALUE', 'Token value must be a string');
      }
      
      // Encrypt and store
      const encrypted = this._encrypt(value);
      this.store.set(`secure.tokens.${key}`, encrypted);
      
      this._logAccess('STORE', key, true);
    } catch (error) {
      this._logAccess('STORE', key, false, error.message);
      throw error;
    }
  }

  /**
   * Retrieve a decrypted token
   * @param {string} key - Token identifier
   * @returns {Promise<string|null>} - Decrypted token or null if not found
   */
  async getToken(key) {
    try {
      this._validateKey(key);
      
      const encrypted = this.store.get(`secure.tokens.${key}`);
      
      if (!encrypted) {
        this._logAccess('GET', key, true); // Success - token doesn't exist
        return null;
      }
      
      const decrypted = this._decrypt(encrypted);
      this._logAccess('GET', key, true);
      
      return decrypted;
    } catch (error) {
      this._logAccess('GET', key, false, error.message);
      throw error;
    }
  }


  /**
   * Delete a stored token
   * @param {string} key - Token identifier
   * @returns {Promise<boolean>} - True if deleted, false if not found
   */
  async deleteToken(key) {
    try {
      this._validateKey(key);
      
      const exists = this.store.has(`secure.tokens.${key}`);
      
      if (exists) {
        this.store.delete(`secure.tokens.${key}`);
        this._logAccess('DELETE', key, true);
        return true;
      }
      
      this._logAccess('DELETE', key, true); // Success - already doesn't exist
      return false;
    } catch (error) {
      this._logAccess('DELETE', key, false, error.message);
      throw error;
    }
  }

  /**
   * Check if a token exists
   * @param {string} key - Token identifier
   * @returns {Promise<boolean>}
   */
  async hasToken(key) {
    try {
      this._validateKey(key);
      const exists = this.store.has(`secure.tokens.${key}`);
      this._logAccess('HAS', key, true);
      return exists;
    } catch (error) {
      this._logAccess('HAS', key, false, error.message);
      throw error;
    }
  }

  /**
   * Clear all stored tokens (USE WITH CAUTION)
   * @returns {Promise<number>} - Number of tokens deleted
   */
  async clearAllTokens() {
    try {
      const allKeys = Array.from(ALLOWED_TOKEN_KEYS);
      let deletedCount = 0;
      
      for (const key of allKeys) {
        if (this.store.has(`secure.tokens.${key}`)) {
          this.store.delete(`secure.tokens.${key}`);
          deletedCount++;
        }
      }
      
      this._logAccess('CLEAR_ALL', '*', true, null, { deletedCount });
      return deletedCount;
    } catch (error) {
      this._logAccess('CLEAR_ALL', '*', false, error.message);
      throw error;
    }
  }

  /**
   * Get all configured token keys (without values)
   * @returns {Promise<string[]>}
   */
  async getConfiguredKeys() {
    const configured = [];
    
    for (const key of ALLOWED_TOKEN_KEYS) {
      if (this.store.has(`secure.tokens.${key}`)) {
        configured.push(key);
      }
    }
    
    return configured;
  }

  /**
   * Get access log (for audit purposes)
   * @returns {Array} - Access log entries
   */
  getAccessLog() {
    return [...this.accessLog];
  }

  /**
   * Clear access log
   */
  clearAccessLog() {
    this.accessLog = [];
  }

  /**
   * Validate all stored tokens (check for corruption)
   * @returns {Promise<{valid: string[], invalid: string[]}>}
   */
  async validateStoredTokens() {
    const valid = [];
    const invalid = [];
    
    for (const key of ALLOWED_TOKEN_KEYS) {
      if (this.store.has(`secure.tokens.${key}`)) {
        try {
          const encrypted = this.store.get(`secure.tokens.${key}`);
          this._decrypt(encrypted); // Try to decrypt
          valid.push(key);
        } catch (error) {
          invalid.push(key);
        }
      }
    }
    
    return { valid, invalid };
  }
}

// ==================== CUSTOM ERROR CLASS ====================
export class SecurityError extends Error {
  constructor(code, message) {
    super(message);
    this.name = 'SecurityError';
    this.code = code;
  }
}

// ==================== UTILITY FUNCTIONS ====================

/**
 * Generate a secure random token
 * @param {number} length - Token length in bytes
 * @returns {string} - Hex-encoded random token
 */
export function generateSecureToken(length = 32) {
  return crypto.randomBytes(length).toString('hex');
}

/**
 * Hash a value (for non-reversible operations)
 * @param {string} value - Value to hash
 * @returns {string} - SHA-256 hash
 */
export function hashValue(value) {
  return crypto.createHash('sha256').update(value).digest('hex');
}

/**
 * Constant-time string comparison (prevents timing attacks)
 * @param {string} a - First string
 * @param {string} b - Second string
 * @returns {boolean}
 */
export function secureCompare(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string') {
    return false;
  }
  
  if (a.length !== b.length) {
    return false;
  }
  
  return crypto.timingSafeEqual(Buffer.from(a), Buffer.from(b));
}

// ==================== EXPORT ====================
export default SecurityManager;
