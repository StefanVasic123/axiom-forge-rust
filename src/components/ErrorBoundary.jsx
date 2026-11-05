import React from 'react';
import { AlertCircle, RotateCcw, Home } from 'lucide-react';

export class ErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  componentDidCatch(error, errorInfo) {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
    this.setState({ errorInfo });
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null, errorInfo: null });
    window.location.hash = '#/';
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-slate-950 text-white flex items-center justify-center p-6">
          <div className="max-w-xl w-full bg-slate-900 border border-rose-500/30 rounded-2xl p-8 shadow-2xl">
            <div className="w-14 h-14 rounded-2xl bg-rose-500/10 border border-rose-500/20 flex items-center justify-center mb-6">
              <AlertCircle className="w-8 h-8 text-rose-400" />
            </div>
            
            <h2 className="text-xl font-bold text-white mb-2">
              Došlo je do neočekivane greške
            </h2>
            <p className="text-sm text-slate-400 mb-4">
              Komponenta se zaustavila. Možete se vratiti na početni Dashboard ili osvežiti aplikaciju.
            </p>

            <div className="bg-slate-950 border border-slate-800 rounded-lg p-4 font-mono text-xs text-rose-300 max-h-48 overflow-auto mb-6 whitespace-pre-wrap">
              {this.state.error?.stack || this.state.error?.message || String(this.state.error)}
            </div>

            <div className="flex items-center gap-3">
              <button
                onClick={this.handleReset}
                className="flex items-center gap-2 px-5 py-2.5 bg-indigo-600 hover:bg-indigo-500 text-white font-medium rounded-xl text-sm transition-colors cursor-pointer"
              >
                <Home className="w-4 h-4" />
                Vrati se na Dashboard
              </button>
              <button
                onClick={() => {
                  this.setState({ hasError: false, error: null, errorInfo: null });
                }}
                className="flex items-center gap-2 px-5 py-2.5 bg-slate-800 hover:bg-slate-700 text-slate-300 font-medium rounded-xl text-sm transition-colors cursor-pointer"
              >
                <RotateCcw className="w-4 h-4" />
                Pokušaj ponovo
              </button>
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary;
