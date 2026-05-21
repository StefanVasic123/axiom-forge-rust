/**
 * Axiom Forge - App Context
 * Global state management using React Context
 */

import React, { createContext, useContext, useReducer, useCallback } from 'react';

// Initial state
const initialState = {
  // App state
  isFirstRun: false,
  isLoading: false,
  error: null,
  
  // Projects
  projects: [],
  currentProject: null,
  
  // Tasks
  activeTasks: [],
  taskProgress: {},
  
  // Settings
  settings: {
    ollamaHost: 'http://127.0.0.1:11434',
    defaultModel: 'llama3.2:1b',
    theme: 'dark',
    autoDeploy: false
  },
  
  // Tokens status
  tokensConfigured: {
    github: false,
    hosting: false
  }
};

// Action types
const ACTIONS = {
  SET_FIRST_RUN: 'SET_FIRST_RUN',
  SET_LOADING: 'SET_LOADING',
  SET_ERROR: 'SET_ERROR',
  SET_PROJECTS: 'SET_PROJECTS',
  ADD_PROJECT: 'ADD_PROJECT',
  UPDATE_PROJECT: 'UPDATE_PROJECT',
  DELETE_PROJECT: 'DELETE_PROJECT',
  SET_CURRENT_PROJECT: 'SET_CURRENT_PROJECT',
  SET_TASK_PROGRESS: 'SET_TASK_PROGRESS',
  UPDATE_SETTINGS: 'UPDATE_SETTINGS',
  SET_TOKENS_CONFIGURED: 'SET_TOKENS_CONFIGURED'
};

// Reducer
function appReducer(state, action) {
  switch (action.type) {
    case ACTIONS.SET_FIRST_RUN:
      return { ...state, isFirstRun: action.payload };
      
    case ACTIONS.SET_LOADING:
      return { ...state, isLoading: action.payload };
      
    case ACTIONS.SET_ERROR:
      return { ...state, error: action.payload };
      
    case ACTIONS.SET_PROJECTS:
      return { ...state, projects: action.payload };
      
    case ACTIONS.ADD_PROJECT:
      return { 
        ...state, 
        projects: [action.payload, ...state.projects] 
      };
      
    case ACTIONS.UPDATE_PROJECT:
      return {
        ...state,
        projects: state.projects.map(p => 
          p.id === action.payload.id ? { ...p, ...action.payload } : p
        )
      };
      
    case ACTIONS.DELETE_PROJECT:
      return {
        ...state,
        projects: state.projects.filter(p => p.id !== action.payload)
      };
      
    case ACTIONS.SET_CURRENT_PROJECT:
      return { ...state, currentProject: action.payload };
      
    case ACTIONS.SET_TASK_PROGRESS:
      return {
        ...state,
        taskProgress: {
          ...state.taskProgress,
          [action.payload.taskId]: action.payload.progress
        }
      };
      
    case ACTIONS.UPDATE_SETTINGS:
      return {
        ...state,
        settings: { ...state.settings, ...action.payload }
      };
      
    case ACTIONS.SET_TOKENS_CONFIGURED:
      return {
        ...state,
        tokensConfigured: { ...state.tokensConfigured, ...action.payload }
      };
      
    default:
      return state;
  }
}

// Context
const AppContext = createContext(null);

// Provider
export function AppProvider({ children }) {
  const [state, dispatch] = useReducer(appReducer, initialState);

  // Actions
  const setFirstRun = useCallback((value) => {
    dispatch({ type: ACTIONS.SET_FIRST_RUN, payload: value });
  }, []);

  const setLoading = useCallback((value) => {
    dispatch({ type: ACTIONS.SET_LOADING, payload: value });
  }, []);

  const setError = useCallback((error) => {
    dispatch({ type: ACTIONS.SET_ERROR, payload: error });
  }, []);

  const setProjects = useCallback((projects) => {
    dispatch({ type: ACTIONS.SET_PROJECTS, payload: projects });
  }, []);

  const addProject = useCallback((project) => {
    dispatch({ type: ACTIONS.ADD_PROJECT, payload: project });
  }, []);

  const updateProject = useCallback((project) => {
    dispatch({ type: ACTIONS.UPDATE_PROJECT, payload: project });
  }, []);

  const deleteProject = useCallback((projectId) => {
    dispatch({ type: ACTIONS.DELETE_PROJECT, payload: projectId });
  }, []);

  const setCurrentProject = useCallback((project) => {
    dispatch({ type: ACTIONS.SET_CURRENT_PROJECT, payload: project });
  }, []);

  const setTaskProgress = useCallback((taskId, progress) => {
    dispatch({ type: ACTIONS.SET_TASK_PROGRESS, payload: { taskId, progress } });
  }, []);

  const updateSettings = useCallback((settings) => {
    dispatch({ type: ACTIONS.UPDATE_SETTINGS, payload: settings });
  }, []);

  const setTokensConfigured = useCallback((tokens) => {
    dispatch({ type: ACTIONS.SET_TOKENS_CONFIGURED, payload: tokens });
  }, []);

  const actions = React.useMemo(() => ({
    setFirstRun,
    setLoading,
    setError,
    setProjects,
    addProject,
    updateProject,
    deleteProject,
    setCurrentProject,
    setTaskProgress,
    updateSettings,
    setTokensConfigured
  }), [
    setFirstRun, setLoading, setError, setProjects,
    addProject, updateProject, deleteProject,
    setCurrentProject, setTaskProgress, updateSettings,
    setTokensConfigured
  ]);

  const value = React.useMemo(() => ({ state, actions }), [state, actions]);

  return (
    <AppContext.Provider value={value}>
      {children}
    </AppContext.Provider>
  );
}

// Hook
export function useAppContext() {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error('useAppContext must be used within AppProvider');
  }
  return context;
}

export default AppContext;
