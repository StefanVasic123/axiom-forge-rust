/**
 * Axiom Forge - Global Configuration
 * 
 * This file centralizes all global constants and configuration values.
 */

export const APP_CONFIG = {
  // Production Cloud URL (Fallback if .env is missing)
  // When publishing as Open Source, this ensures the app works out-of-the-box.
  // Developers can still override this in their local .env file.
  IDEA_ANALYZER_URL: process.env.IDEA_ANALYZER_URL || 'https://idea-analyzer-ten.vercel.app',
  
  // App Metadata
  VERSION: '1.0.0',
  APP_NAME: 'Axiom Forge',
  
  // Protocol Schema
  PROTOCOL: 'axiom'
};

export default APP_CONFIG;
