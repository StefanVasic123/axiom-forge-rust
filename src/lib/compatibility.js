export const COMPATIBILITY_RULES = {
  nextjs: {
    "15": { node: ">=18.17.0", react: ">=19.0.0" },
    "14": { node: ">=18.17.0", react: ">=18.0.0" },
    "13": { node: ">=16.8.0", react: ">=18.0.0" },
    "12": { node: ">=12.22.0", react: ">=17.0.0" }
  },
  tauri: {
    "2": { rust: ">=1.77.2", node: ">=18.0.0" },
    "1": { rust: ">=1.63.0", node: ">=16.0.0" }
  },
  expo: {
    "51": { node: ">=18.0.0" },
    "50": { node: ">=18.0.0" }
  },
  features: {
    "auth": { node: ">=18.0.0", description: "Authentication packages (NextAuth/Auth.js) require Node 18+" }
  }
};

/**
 * Parses semver string to an array of numbers [major, minor, patch]
 */
function parseVersion(vStr) {
  if (!vStr) return [0, 0, 0];
  const cleaned = vStr.replace(/^v/, '').trim();
  const parts = cleaned.split('.').map(p => parseInt(p, 10) || 0);
  while (parts.length < 3) parts.push(0);
  return parts.slice(0, 3);
}

/**
 * Checks if version satisfies constraint (e.g. ">=18.17.0")
 */
export function satisfies(version, constraint) {
  if (!version || !constraint) return true;
  const opMatch = constraint.match(/^([>=<]+)?\s*(.*)$/);
  if (!opMatch) return true;
  
  const op = opMatch[1] || '==';
  const targetStr = opMatch[2];
  
  const vParts = parseVersion(version);
  const tParts = parseVersion(targetStr);
  
  // Compare arrays
  for (let i = 0; i < 3; i++) {
    if (vParts[i] !== tParts[i]) {
      const diff = vParts[i] - tParts[i];
      if (op === '>=') return diff > 0;
      if (op === '<=') return diff < 0;
      if (op === '>') return diff > 0;
      if (op === '<') return diff < 0;
      if (op === '==' || op === '=') return false;
    }
  }
  return op.includes('=') || op === '==';
}

/**
 * Verifies compatibility of selected versions against local tools
 * @param {Object} selectedOverrides e.g. { nextjs: '14', tauri: '2' }
 * @param {Object} localTools e.g. { node: '20.5.0', rust: '1.75.0' }
 * @returns {Array} List of warning messages
 */
export function verifyCompatibility(selectedOverrides, localTools) {
  const warnings = [];
  if (!selectedOverrides || !localTools) return warnings;

  // Next.js checks
  if (selectedOverrides.nextjs && COMPATIBILITY_RULES.nextjs[selectedOverrides.nextjs]) {
    const rules = COMPATIBILITY_RULES.nextjs[selectedOverrides.nextjs];
    if (rules.node && localTools.node && !satisfies(localTools.node, rules.node)) {
      warnings.push(`Next.js v${selectedOverrides.nextjs} requires Node ${rules.node}, but you have v${localTools.node}.`);
    }
    if (selectedOverrides.react && rules.react && !satisfies(selectedOverrides.react, rules.react)) {
      warnings.push(`Next.js v${selectedOverrides.nextjs} requires React ${rules.react}, but React v${selectedOverrides.react} is selected.`);
    }
  }

  // Tauri checks
  if (selectedOverrides.tauri && COMPATIBILITY_RULES.tauri[selectedOverrides.tauri]) {
    const rules = COMPATIBILITY_RULES.tauri[selectedOverrides.tauri];
    if (rules.rust && localTools.rust && !satisfies(localTools.rust, rules.rust)) {
      warnings.push(`Tauri v${selectedOverrides.tauri} requires Rust ${rules.rust}, but you have v${localTools.rust}.`);
    }
    if (rules.node && localTools.node && !satisfies(localTools.node, rules.node)) {
      warnings.push(`Tauri v${selectedOverrides.tauri} requires Node ${rules.node}, but you have v${localTools.node}.`);
    }
  }

  // Expo checks
  if (selectedOverrides.expo && COMPATIBILITY_RULES.expo[selectedOverrides.expo]) {
    const rules = COMPATIBILITY_RULES.expo[selectedOverrides.expo];
    if (rules.node && localTools.node && !satisfies(localTools.node, rules.node)) {
      warnings.push(`Expo SDK ${selectedOverrides.expo} requires Node ${rules.node}, but you have v${localTools.node}.`);
    }
  }

  return warnings;
}
