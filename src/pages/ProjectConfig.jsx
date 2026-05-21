/**
 * Axiom Forge - Project Configuration
 * UI for entering ENV variables with help tooltips for API credentials
 */

import React, { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { 
  ArrowLeft, 
  Save, 
  ExternalLink, 
  HelpCircle, 
  Eye, 
  EyeOff, 
  CheckCircle2,
  AlertCircle,
  Loader2,
  Key,
  Shield
} from 'lucide-react';
import { useAppStore } from '../hooks/useAppStore';

// Environment variable field with help tooltip
function EnvField({ 
  name, 
  label, 
  description, 
  helpUrl, 
  required, 
  value, 
  onChange,
  example,
  type = 'text'
}) {
  const [showValue, setShowValue] = useState(false);
  const [isFocused, setIsFocused] = useState(false);

  const isSecret = type === 'password' || name.toLowerCase().includes('secret') || 
                   name.toLowerCase().includes('key') || name.toLowerCase().includes('token');

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <label className="label flex items-center gap-2">
          <Key className="w-4 h-4 text-slate-500" />
          {label || name}
          {required && <span className="text-rose-400">*</span>}
        </label>
        
        <div className="flex items-center gap-2">
          {helpUrl && (
            <a
              href={helpUrl}
              onClick={(e) => {
                e.preventDefault();
                window.electronAPI.shell.openExternal(helpUrl);
              }}
              className="flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300"
            >
              <HelpCircle className="w-3 h-3" />
              How to get this
            </a>
          )}
        </div>
      </div>
      
      {description && (
        <p className="text-xs text-slate-500">{description}</p>
      )}
      
      <div className="relative">
        <input
          type={isSecret && !showValue ? 'password' : 'text'}
          value={value}
          onChange={(e) => onChange(name, e.target.value)}
          onFocus={() => setIsFocused(true)}
          onBlur={() => setIsFocused(false)}
          placeholder={example || `Enter ${label || name}`}
          className={`input pr-10 transition-all ${
            isFocused ? 'ring-2 ring-indigo-500/50 border-indigo-500' : ''
          }`}
        />
        
        {isSecret && (
          <button
            onClick={() => setShowValue(!showValue)}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition-colors"
            type="button"
          >
            {showValue ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
          </button>
        )}
      </div>
      
      {example && (
        <p className="text-xs text-slate-600">
          Example: <code className="bg-slate-900 px-1.5 py-0.5 rounded text-slate-500">{example}</code>
        </p>
      )}
    </div>
  );
}

// Preset configurations for common services
const SERVICE_PRESETS = {
  google: {
    name: 'Google OAuth',
    icon: '🔐',
    fields: [
      {
        name: 'GOOGLE_CLIENT_ID',
        label: 'Client ID',
        description: 'OAuth 2.0 Client ID from Google Cloud Console',
        helpUrl: 'https://console.cloud.google.com/apis/credentials',
        required: true,
        example: '123456789-abc123def456.apps.googleusercontent.com'
      },
      {
        name: 'GOOGLE_CLIENT_SECRET',
        label: 'Client Secret',
        description: 'OAuth 2.0 Client Secret',
        helpUrl: 'https://console.cloud.google.com/apis/credentials',
        required: true,
        type: 'password'
      }
    ]
  },
  stripe: {
    name: 'Stripe',
    icon: '💳',
    fields: [
      {
        name: 'STRIPE_PUBLISHABLE_KEY',
        label: 'Publishable Key',
        description: 'Your Stripe publishable key (starts with pk_)',
        helpUrl: 'https://dashboard.stripe.com/apikeys',
        required: true,
        example: 'pk_test_...'
      },
      {
        name: 'STRIPE_SECRET_KEY',
        label: 'Secret Key',
        description: 'Your Stripe secret key (starts with sk_)',
        helpUrl: 'https://dashboard.stripe.com/apikeys',
        required: true,
        type: 'password'
      }
    ]
  },
  openai: {
    name: 'OpenAI',
    icon: '🤖',
    fields: [
      {
        name: 'OPENAI_API_KEY',
        label: 'API Key',
        description: 'Your OpenAI API key',
        helpUrl: 'https://platform.openai.com/api-keys',
        required: true,
        type: 'password',
        example: 'sk-...'
      }
    ]
  },
  supabase: {
    name: 'Supabase',
    icon: '⚡',
    fields: [
      {
        name: 'SUPABASE_URL',
        label: 'Project URL',
        description: 'Your Supabase project URL',
        helpUrl: 'https://app.supabase.com/project/_/settings/api',
        required: true,
        example: 'https://xxxxxx.supabase.co'
      },
      {
        name: 'SUPABASE_ANON_KEY',
        label: 'Anon Key',
        description: 'Your Supabase anon/public key',
        helpUrl: 'https://app.supabase.com/project/_/settings/api',
        required: true,
        type: 'password'
      }
    ]
  }
};

// Main ProjectConfig Component
function ProjectConfig() {
  const { projectId } = useParams();
  const navigate = useNavigate();
  const { projects, configureProject, updateProjectStatus } = useAppStore();
  
  const [project, setProject] = useState(null);
  const [envVars, setEnvVars] = useState({});
  const [customVars, setCustomVars] = useState([{ key: '', value: '' }]);
  const [isSaving, setIsSaving] = useState(false);
  const [saveError, setSaveError] = useState(null);
  const [saveSuccess, setSaveSuccess] = useState(false);
  const [activePreset, setActivePreset] = useState(null);

  // Load project data
  useEffect(() => {
    const foundProject = projects.find(p => p.id === projectId);
    if (foundProject) {
      setProject(foundProject);
      
      // Initialize env vars from template if available
      if (foundProject.metadata?.envTemplate) {
        const initialVars = {};
        foundProject.metadata.envTemplate.forEach(env => {
          initialVars[env.name] = '';
        });
        setEnvVars(initialVars);
      }
    }
  }, [projectId, projects]);

  // Handle env var change
  const handleEnvChange = (name, value) => {
    setEnvVars(prev => ({ ...prev, [name]: value }));
    setSaveSuccess(false);
  };

  // Handle custom var change
  const handleCustomVarChange = (index, field, value) => {
    const updated = [...customVars];
    updated[index][field] = value;
    setCustomVars(updated);
  };

  // Add custom var field
  const addCustomVar = () => {
    setCustomVars([...customVars, { key: '', value: '' }]);
  };

  // Remove custom var field
  const removeCustomVar = (index) => {
    setCustomVars(customVars.filter((_, i) => i !== index));
  };

  // Apply preset
  const applyPreset = (presetKey) => {
    setActivePreset(presetKey);
    const preset = SERVICE_PRESETS[presetKey];
    
    // Initialize fields for this preset
    const presetVars = {};
    preset.fields.forEach(field => {
      presetVars[field.name] = envVars[field.name] || '';
    });
    
    setEnvVars(prev => ({ ...prev, ...presetVars }));
  };

  // Handle save
  const handleSave = async () => {
    setIsSaving(true);
    setSaveError(null);
    setSaveSuccess(false);

    try {
      // Combine preset vars and custom vars
      const allVars = { ...envVars };
      
      // Add custom vars
      customVars.forEach(({ key, value }) => {
        if (key.trim()) {
          allVars[key.trim()] = value;
        }
      });

      // Save to secure storage
      await configureProject(projectId, allVars);
      
      // Update project status and metadata
      const updatedMetadata = {
        ...project.metadata,
        configuredAt: new Date().toISOString(),
        envVarCount: Object.keys(allVars).length,
        deployment: project.metadata?.deployment // This now contains the selected provider
      };

      await updateProjectStatus(projectId, 'ready', updatedMetadata);

      setSaveSuccess(true);
      
      // Navigate back after short delay
      setTimeout(() => {
        navigate(`/projects/${projectId}`);
      }, 1500);
    } catch (error) {
      setSaveError(error.message);
    } finally {
      setIsSaving(false);
    }
  };

  // Handle deploy
  const handleDeploy = async () => {
    await handleSave();
    
    // Start deployment
    try {
      await window.electronAPI.task.startDeployment(projectId);
      navigate('/');
    } catch (error) {
      setSaveError(error.message);
    }
  };

  if (!project) {
    return (
      <div className="flex items-center justify-center py-16">
        <Loader2 className="w-8 h-8 text-indigo-500 animate-spin" />
      </div>
    );
  }

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-center gap-4">
        <button
          onClick={() => navigate(-1)}
          className="p-2 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </button>
        <div>
          <h1 className="text-2xl font-bold text-white">Configure Project</h1>
          <p className="text-slate-400">Set up environment variables for {project.name}</p>
        </div>
      </div>

      {/* Security Notice */}
      <div className="card p-4 border-indigo-500/20 bg-indigo-500/5">
        <div className="flex items-start gap-3">
          <Shield className="w-5 h-5 text-indigo-400 flex-shrink-0 mt-0.5" />
          <div>
            <p className="text-sm text-indigo-300 font-medium">Your data is secure</p>
            <p className="text-xs text-indigo-400/70">
              All environment variables are encrypted and stored locally. 
              They are only used during deployment and never sent to our servers.
            </p>
          </div>
        </div>
      </div>

      {/* Hosting Provider */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-white mb-4">Hosting Provider</h2>
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
          {[
            { id: 'vercel', name: 'Vercel', icon: <Globe className="w-4 h-4" /> },
            { id: 'netlify', name: 'Netlify', icon: <ExternalLink className="w-4 h-4" /> },
            { id: 'render', name: 'Render', icon: <Server className="w-4 h-4" /> },
            { id: 'hostinger', name: 'Hostinger', icon: <Shield className="w-4 h-4" /> }
          ].map((prov) => (
            <button
              key={prov.id}
              onClick={() => {
                setProject(prev => ({
                  ...prev,
                  metadata: {
                    ...prev.metadata,
                    deployment: { ...prev.metadata?.deployment, provider: prov.id }
                  }
                }));
              }}
              className={`flex flex-col items-center justify-center p-4 rounded-xl border transition-all gap-2 ${
                project.metadata?.deployment?.provider === prov.id
                  ? 'bg-emerald-600/20 border-emerald-500 text-emerald-400'
                  : 'bg-slate-800/50 border-slate-700 text-slate-400 hover:border-slate-600'
              }`}
            >
              {prov.icon}
              <span className="text-xs font-bold">{prov.name}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Service Presets */}
      <div className="card p-6">
        <h2 className="text-lg font-semibold text-white mb-4">Quick Add Services</h2>
        <div className="flex flex-wrap gap-2">
          {Object.entries(SERVICE_PRESETS).map(([key, preset]) => (
            <button
              key={key}
              onClick={() => applyPreset(key)}
              className={`flex items-center gap-2 px-4 py-2 rounded-lg border transition-all ${
                activePreset === key
                  ? 'bg-indigo-600/20 border-indigo-500 text-indigo-400'
                  : 'bg-slate-800/50 border-slate-700 text-slate-300 hover:border-slate-600'
              }`}
            >
              <span>{preset.icon}</span>
              <span className="text-sm font-medium">{preset.name}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Environment Variables Form */}
      <div className="card p-6 space-y-6">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold text-white">Environment Variables</h2>
          <span className="text-xs text-slate-500">
            {Object.keys(envVars).length} variables
          </span>
        </div>

        {/* Preset Fields */}
        {activePreset && SERVICE_PRESETS[activePreset] && (
          <div className="space-y-4 pb-6 border-b border-slate-800">
            <h3 className="text-sm font-medium text-slate-400 flex items-center gap-2">
              {SERVICE_PRESETS[activePreset].icon}
              {SERVICE_PRESETS[activePreset].name}
            </h3>
            {SERVICE_PRESETS[activePreset].fields.map((field) => (
              <EnvField
                key={field.name}
                {...field}
                value={envVars[field.name] || ''}
                onChange={handleEnvChange}
              />
            ))}
          </div>
        )}

        {/* Template Fields */}
        {project.metadata?.envTemplate && project.metadata.envTemplate.length > 0 && (
          <div className="space-y-4">
            <h3 className="text-sm font-medium text-slate-400">Required Variables</h3>
            {project.metadata.envTemplate.map((env) => (
              <EnvField
                key={env.name}
                name={env.name}
                label={env.label || env.name}
                description={env.description}
                helpUrl={env.helpUrl}
                required={env.required}
                example={env.example}
                value={envVars[env.name] || ''}
                onChange={handleEnvChange}
              />
            ))}
          </div>
        )}

        {/* Custom Variables */}
        <div className="space-y-4 pt-6 border-t border-slate-800">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-slate-400">Custom Variables</h3>
            <button
              onClick={addCustomVar}
              className="text-xs text-indigo-400 hover:text-indigo-300"
            >
              + Add Variable
            </button>
          </div>
          
          {customVars.map((customVar, index) => (
            <div key={index} className="flex gap-3">
              <input
                type="text"
                placeholder="VARIABLE_NAME"
                value={customVar.key}
                onChange={(e) => handleCustomVarChange(index, 'key', e.target.value)}
                className="input flex-1"
              />
              <input
                type="password"
                placeholder="value"
                value={customVar.value}
                onChange={(e) => handleCustomVarChange(index, 'value', e.target.value)}
                className="input flex-1"
              />
              {customVars.length > 1 && (
                <button
                  onClick={() => removeCustomVar(index)}
                  className="p-2 text-slate-500 hover:text-rose-400 transition-colors"
                >
                  ×
                </button>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Alerts */}
      {saveError && (
        <div className="p-4 bg-rose-500/10 border border-rose-500/20 rounded-lg">
          <div className="flex items-start gap-3">
            <AlertCircle className="w-5 h-5 text-rose-400 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-rose-400">{saveError}</p>
          </div>
        </div>
      )}

      {saveSuccess && (
        <div className="p-4 bg-emerald-500/10 border border-emerald-500/20 rounded-lg">
          <div className="flex items-start gap-3">
            <CheckCircle2 className="w-5 h-5 text-emerald-400 flex-shrink-0 mt-0.5" />
            <p className="text-sm text-emerald-400">
              Configuration saved successfully! Redirecting...
            </p>
          </div>
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center justify-end gap-3">
        <button
          onClick={() => navigate(-1)}
          className="btn-ghost"
          disabled={isSaving}
        >
          Cancel
        </button>
        <button
          onClick={handleSave}
          disabled={isSaving}
          className="btn-secondary"
        >
          {isSaving ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Saving...
            </>
          ) : (
            <>
              <Save className="w-4 h-4" />
              Save Configuration
            </>
          )}
        </button>
        <button
          onClick={handleDeploy}
          disabled={isSaving}
          className="btn-primary"
        >
          {isSaving ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              Saving...
            </>
          ) : (
            <>
              <ExternalLink className="w-4 h-4" />
              Save & Deploy
            </>
          )}
        </button>
      </div>
    </div>
  );
}

export default ProjectConfig;
