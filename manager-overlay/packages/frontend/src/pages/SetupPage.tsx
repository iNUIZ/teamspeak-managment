import { translateUiError as uiError } from '@/lib/i18n';
import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Card, CardContent, CardHeader } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Loader2, AlertCircle } from 'lucide-react';
import axios from 'axios';
import { BrandMark } from '@/components/shared/BrandMark';

export default function SetupPage() {
  useUiLanguageStore(state => state.language);
  const navigate = useNavigate();
  const [username, setUsername] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    axios.get('/api/setup/status').then((res) => {
      if (!res.data.needsSetup) navigate('/login', { replace: true });
      else setChecking(false);
    }).catch(() => setChecking(false));
  }, [navigate]);

  if (checking) {
    return (
      <div className="auth-viewport min-h-screen min-h-dvh flex items-center justify-center bg-background">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');

    if (password !== confirmPassword) {
      setError(uiText("Passwords do not match"));
      return;
    }

    setLoading(true);
    try {
      await axios.post('/api/setup/init', {
        username,
        password,
        displayName: displayName || username,
      });
      navigate('/login', { replace: true });
    } catch (err: any) {
      setError(uiError(String(err.response?.data?.error || uiText("Setup failed")), uiText("Setup failed")));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="auth-viewport min-h-screen min-h-dvh flex items-center justify-center bg-background grid-bg">
      <div className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] bg-primary/5 rounded-full blur-[120px] pointer-events-none" />

      <div className="w-full max-w-sm mx-4 relative">
        <div className="text-center mb-8">
          <BrandMark className="mx-auto mb-4 h-16 w-16 text-primary" />
          <h1 className="text-xl font-semibold text-foreground">{uiText("Teamspeak Managment")}</h1>
          <p className="text-sm text-muted-foreground mt-1">{uiText("Initial setup · Create your administrator account")}</p>
        </div>

        <Card className="border-border/50 backdrop-blur-sm">
          <CardHeader className="pb-4">
            <h2 className="text-sm font-medium text-center text-muted-foreground">
              {uiText("This is the first time setup. Create an admin account to get started.")}</h2>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="username" className="text-xs">{uiText("Username")}</Label>
                <Input
                  id="username"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder={uiText("admin")}
                  autoComplete="username"
                  autoFocus
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="displayName" className="text-xs">{uiText("Display Name")}</Label>
                <Input
                  id="displayName"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  placeholder={uiText("Administrator")}
                  autoComplete="name"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="password" className="text-xs">{uiText("Password")}</Label>
                <Input
                  id="password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={uiText("Min. 8 chars, uppercase, lowercase, digit")}
                  autoComplete="new-password"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="confirmPassword" className="text-xs">{uiText("Confirm Password")}</Label>
                <Input
                  id="confirmPassword"
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  placeholder={uiText("Repeat password")}
                  autoComplete="new-password"
                />
              </div>

              {error && (
                <div className="flex items-center gap-2 text-destructive text-xs bg-destructive/10 rounded-md px-3 py-2">
                  <AlertCircle className="h-3.5 w-3.5 shrink-0" />
                  <span>{error}</span>
                </div>
              )}

              <Button
                type="submit"
                className="w-full"
                disabled={loading || !username || !password || !confirmPassword}
              >
                {loading ? (
                  <>
                    <Loader2 className="h-4 w-4 animate-spin" />
                    {uiText("Creating account...")}</>
                ) : (
                  uiText("Create Admin Account")
                )}
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
