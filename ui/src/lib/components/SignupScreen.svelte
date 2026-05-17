<script lang="ts">
  type SignupStep = 'form' | 'sent' | 'verified';

  interface Props {
    onCreateAccount: (email: string) => Promise<{ ok: boolean; error?: string }>;
    onGoToLogin: () => void;
    signupBusy: boolean;
    signupError: string;
  }

  let {
    onCreateAccount,
    onGoToLogin,
    signupBusy = false,
    signupError = ''
  }: Props = $props();

  let email = $state('');
  let step = $state<SignupStep>('form');
  let resendTimer = $state(0);
  let timerInterval: ReturnType<typeof setInterval> | null = null;

  function isValidEmail(e: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e);
  }

  async function handleSubmit() {
    if (!isValidEmail(email)) return;
    const result = await onCreateAccount(email);
    if (result.ok) {
      step = 'sent';
      startResendTimer();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !signupBusy && isValidEmail(email)) {
      handleSubmit();
    }
  }

  function startResendTimer() {
    resendTimer = 30;
    if (timerInterval) clearInterval(timerInterval);
    timerInterval = setInterval(() => {
      resendTimer -= 1;
      if (resendTimer <= 0) {
        if (timerInterval) clearInterval(timerInterval);
        timerInterval = null;
      }
    }, 1000);
  }

  async function handleResend() {
    if (resendTimer > 0) return;
    await onCreateAccount(email);
    startResendTimer();
  }
</script>

<div class="signup-page">
  {#if step === 'form'}
    <div class="signup-header">
      <div class="signup-icon">&#9889;</div>
      <div class="signup-title">Create Free Account</div>
      <div class="signup-desc">Get started with PrivadoVPN — no credit card required</div>
    </div>

    <div class="signup-form card">
      <div class="form-group">
        <label for="signup-email" class="form-label">Email Address</label>
        <input
          id="signup-email"
          type="email"
          bind:value={email}
          placeholder="you@example.com"
          class="form-input"
          disabled={signupBusy}
          onkeydown={handleKeyDown}
        />
      </div>

      {#if signupError}
        <div class="error-message">
          <span class="error-icon">&#9888;&#65039;</span>
          <span>{signupError}</span>
        </div>
      {/if}

      <button
        class="btn-signup"
        onclick={handleSubmit}
        disabled={signupBusy || !isValidEmail(email)}
      >
        {signupBusy ? 'Creating account...' : 'Create Account'}
      </button>
    </div>

    <div class="login-prompt">
      <span class="login-text">Already have an account?</span>
      <button class="login-link" onclick={onGoToLogin}>Log In</button>
    </div>

    <div class="features-card card">
      <div class="feature-title">Free Plan Includes</div>
      <div class="feature-item">
        <span class="feature-check">&#10003;</span>
        <span>10GB monthly data</span>
      </div>
      <div class="feature-item">
        <span class="feature-check">&#10003;</span>
        <span>12 server locations</span>
      </div>
      <div class="feature-item">
        <span class="feature-check">&#10003;</span>
        <span>Kill switch protection</span>
      </div>
      <div class="feature-item">
        <span class="feature-check">&#10003;</span>
        <span>No ads, no logs</span>
      </div>
    </div>

  {:else if step === 'sent'}
    <div class="sent-container">
      <div class="sent-icon">&#9993;&#65039;</div>
      <div class="sent-title">Check Your Email</div>
      <div class="sent-desc">
        We sent login credentials to <strong>{email}</strong>. Check your inbox (and spam folder) for your username and password.
      </div>

      <div class="sent-actions">
        <button
          class="btn-resend"
          onclick={handleResend}
          disabled={resendTimer > 0 || signupBusy}
        >
          {#if resendTimer > 0}
            Resend in {resendTimer}s
          {:else}
            Resend Email
          {/if}
        </button>

        <button class="btn-go-login" onclick={onGoToLogin}>
          I have my credentials — Log In
        </button>
      </div>

      <div class="sent-help">
        <div class="help-icon">&#8505;&#65039;</div>
        <div class="help-text">
          Your email is your username. The password will be in the email. You can change it later at privado.io.
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .signup-page {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 20px 0;
  }

  .signup-header {
    text-align: center;
  }

  .signup-icon {
    font-size: 48px;
    display: block;
    margin-bottom: 12px;
  }

  .signup-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
    margin-bottom: 6px;
  }

  .signup-desc {
    font-size: 13px;
    color: var(--text-muted);
  }

  .signup-form {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .form-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .form-input {
    padding: 10px 14px;
    font-size: 14px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
  }

  .form-input:focus {
    border-color: var(--orange-500);
  }

  .form-input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error-message {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 12px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--red-500);
    border-radius: var(--radius-md);
    color: var(--red-500);
    font-size: 13px;
    margin-bottom: 12px;
  }

  .error-icon {
    flex-shrink: 0;
  }

  .btn-signup {
    width: 100%;
    padding: 12px 24px;
    background: var(--orange-500);
    border: none;
    border-radius: var(--radius-pill);
    color: white;
    font-weight: 600;
    font-size: 16px;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
  }

  .btn-signup:hover:not(:disabled) { background: var(--orange-400); }
  .btn-signup:active:not(:disabled) { background: var(--orange-600); transform: scale(0.98); }
  .btn-signup:disabled { background: var(--text-disabled); cursor: not-allowed; }

  .login-prompt {
    text-align: center;
    font-size: 13px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .login-link {
    color: var(--orange-500);
    font-weight: 600;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 13px;
    padding: 0;
  }

  .login-link:hover { color: var(--orange-400); }

  .features-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .feature-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 12px;
  }

  .feature-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    font-size: 14px;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
  }

  .feature-item:last-child { border-bottom: none; }

  .feature-check {
    color: var(--green-500);
    font-weight: 700;
    font-size: 16px;
    flex-shrink: 0;
    width: 20px;
    text-align: center;
  }

  .sent-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    text-align: center;
    padding: 20px 0;
  }

  .sent-icon { font-size: 64px; }

  .sent-title {
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .sent-desc {
    font-size: 14px;
    color: var(--text-dim);
    line-height: 1.5;
    max-width: 320px;
  }

  .sent-actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    margin-top: 8px;
  }

  .btn-resend {
    width: 100%;
    padding: 12px 24px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    color: var(--text-dim);
    font-weight: 600;
    font-size: 14px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .btn-resend:hover:not(:disabled) { border-color: var(--orange-500); color: var(--orange-500); }
  .btn-resend:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-go-login {
    width: 100%;
    padding: 14px 24px;
    background: var(--orange-500);
    border: none;
    border-radius: var(--radius-pill);
    color: white;
    font-weight: 600;
    font-size: 16px;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-go-login:hover { background: var(--orange-400); }

  .sent-help {
    display: flex;
    gap: 12px;
    padding: var(--sp-4);
    background: rgba(223, 92, 5, 0.1);
    border: 1px solid var(--orange-500);
    border-radius: var(--radius-md);
    width: 100%;
    text-align: left;
    margin-top: 8px;
  }

  .help-icon { font-size: 18px; flex-shrink: 0; }

  .help-text {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.4;
  }
</style>
