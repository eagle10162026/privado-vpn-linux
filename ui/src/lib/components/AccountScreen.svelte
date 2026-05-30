<script lang="ts">
  interface Props {
    isLoggedIn: boolean;
    username: string;
    accountType: 'Free' | 'Premium';
    loginBusy: boolean;
    loginError: string;
    onLogin: (username: string, password: string) => void;
    onLogout: () => void;
  }

  let {
    isLoggedIn = false,
    username = '',
    accountType = 'Premium',
    loginBusy = false,
    loginError = '',
    onLogin,
    onLogout
  }: Props = $props();

  let loginUsername = $state('');
  let loginPassword = $state('');
  let showLogoutConfirm = $state(false);

  function handleLogin() {
    if (loginUsername.trim() && loginPassword.trim()) {
      onLogin(loginUsername, loginPassword);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !loginBusy) {
      handleLogin();
    }
  }

  function handleLogout() {
    showLogoutConfirm = false;
    onLogout();
  }

  function getAvatarInitial(): string {
    return username.charAt(0).toUpperCase();
  }
</script>

<div class="account-page">
  {#if isLoggedIn}
    <div class="logged-in-container">
      <div class="avatar">{getAvatarInitial()}</div>

      <div class="user-info">
        <div class="username">{username}</div>
        <div class="login-status">✓ Logged in</div>
      </div>

      <div class="account-card card">
        <div class="card-section">
          <div class="card-label">Account Type</div>
          <div class="card-value accent">{accountType}</div>
        </div>

        <div class="divider"></div>

        <div class="card-section">
          <div class="card-label">Protocol</div>
          <div class="card-value">IKEv2/IPsec</div>
        </div>

        <div class="divider"></div>

        <div class="card-section">
          <div class="card-label">Engine</div>
          <div class="card-value">strongSwan</div>
        </div>

        <div class="divider"></div>

        <div class="card-section">
          <div class="card-label">Status</div>
          <div class="card-value">Active</div>
        </div>
      </div>

      <div class="stats-container">
        <div class="stat-card">
          <div class="stat-icon">📊</div>
          <div class="stat-label">Total Data</div>
          <div class="stat-value">2.4 GB</div>
        </div>
        <div class="stat-card">
          <div class="stat-icon">⏱️</div>
          <div class="stat-label">Sessions</div>
          <div class="stat-value">47</div>
        </div>
      </div>

      {#if showLogoutConfirm}
        <div class="logout-confirm">
          <div class="confirm-text">Log out of your account?</div>
          <div class="confirm-buttons">
            <button
              class="btn-cancel"
              onclick={() => (showLogoutConfirm = false)}
            >
              Cancel
            </button>
            <button
              class="btn-confirm-logout"
              onclick={handleLogout}
            >
              Log Out
            </button>
          </div>
        </div>
      {:else}
        <button
          class="btn-logout"
          onclick={() => (showLogoutConfirm = true)}
        >
          Log Out
        </button>
      {/if}
    </div>
  {:else}
    <div class="login-container">
      <div class="login-header">
        <div class="login-icon">🔐</div>
        <div class="login-title">Log In</div>
        <div class="login-desc">Use your Privado username (not email)</div>
      </div>

      <div class="login-form card">
        <div class="form-group">
          <label class="form-label" for="login-username">Username</label>
          <input
            id="login-username"
            type="text"
            bind:value={loginUsername}
            placeholder="your username"
            class="form-input"
            disabled={loginBusy}
          />
        </div>

        <div class="form-group">
          <label class="form-label" for="login-password">Password</label>
          <input
            id="login-password"
            type="password"
            bind:value={loginPassword}
            placeholder="your password"
            class="form-input"
            disabled={loginBusy}
            onkeydown={handleKeyDown}
          />
        </div>

        {#if loginError}
          <div class="error-message">
            <span class="error-icon">⚠️</span>
            <span>{loginError}</span>
          </div>
        {/if}

        <button
          class="btn-login"
          onclick={handleLogin}
          disabled={loginBusy || !loginUsername.trim() || !loginPassword.trim()}
        >
          {loginBusy ? 'Logging in...' : 'Log In'}
        </button>
      </div>

      <div class="signup-prompt">
        <span class="signup-text">Don't have an account?</span>
        <a href="https://www.privado.io" class="signup-link" target="_blank" rel="noopener noreferrer">
          Create one
        </a>
      </div>

      <div class="info-box">
        <div class="info-icon">ℹ️</div>
        <div class="info-text">
          Your Privado account unlocks access to our global server network and premium features.
        </div>
      </div>
    </div>
  {/if}

  <div class="about-card card">
    <div class="section-title">About</div>

    <div class="about-row">
      <div class="about-label">Version</div>
      <div class="about-value">1.0.0</div>
    </div>

    <div class="about-row">
      <div class="about-label">License</div>
      <div class="about-value">MIT Open Source</div>
    </div>

    <div class="about-row">
      <div class="about-label">Website</div>
      <a href="https://www.privado.io" class="about-link" target="_blank" rel="noopener noreferrer">
        privado.io
      </a>
    </div>
  </div>
</div>

<style>
  .account-page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 0 20px 0;
  }

  .logged-in-container {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .avatar {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: var(--orange-500);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    font-weight: 700;
    color: white;
    margin: 0 auto;
  }

  .user-info {
    text-align: center;
  }

  .username {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .login-status {
    font-size: 13px;
    color: var(--green-500);
    margin-top: 4px;
    font-weight: 500;
  }

  .account-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 0;
    overflow: hidden;
  }

  .card-section {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--sp-4);
  }

  .card-label {
    font-size: 13px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }

  .card-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-dim);
  }

  .card-value.accent {
    color: var(--orange-500);
  }

  .divider {
    height: 1px;
    background: var(--border);
  }

  .stats-container {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .stat-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-4);
    text-align: center;
  }

  .stat-icon {
    font-size: 32px;
    margin-bottom: 8px;
  }

  .stat-label {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    margin-bottom: 6px;
  }

  .stat-value {
    font-size: 18px;
    font-weight: 700;
    color: var(--orange-500);
  }

  .logout-confirm {
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid var(--red-500);
    border-radius: var(--radius-lg);
    padding: var(--sp-4);
  }

  .confirm-text {
    font-size: 14px;
    color: var(--text-primary);
    text-align: center;
  }

  .confirm-buttons {
    display: flex;
    gap: 8px;
  }

  .btn-cancel {
    flex: 1;
    padding: 10px 16px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-cancel:hover {
    background: var(--bg-hover);
  }

  .btn-confirm-logout {
    flex: 1;
    padding: 10px 16px;
    background: var(--red-500);
    border: none;
    border-radius: var(--radius-md);
    color: white;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .btn-confirm-logout:hover {
    background: #dc2626;
  }

  .btn-logout {
    width: 100%;
    padding: 14px 32px;
    background: var(--red-500);
    border: none;
    border-radius: var(--radius-pill);
    color: white;
    font-weight: 600;
    font-size: 16px;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
  }

  .btn-logout:hover {
    background: #dc2626;
  }

  .btn-logout:active {
    transform: scale(0.98);
  }

  .login-container {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .login-header {
    text-align: center;
  }

  .login-icon {
    font-size: 48px;
    display: block;
    margin-bottom: 12px;
  }

  .login-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: 4px;
  }

  .login-desc {
    font-size: 13px;
    color: var(--text-muted);
  }

  .login-form {
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

  .form-group:last-of-type {
    margin-bottom: 12px;
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

  .btn-login {
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

  .btn-login:hover:not(:disabled) {
    background: var(--orange-400);
  }

  .btn-login:active:not(:disabled) {
    background: var(--orange-600);
    transform: scale(0.98);
  }

  .btn-login:disabled {
    background: var(--text-disabled);
    cursor: not-allowed;
  }

  .signup-prompt {
    text-align: center;
    font-size: 13px;
    color: var(--text-muted);
  }

  .signup-text {
    margin-right: 4px;
  }

  .signup-link {
    color: var(--orange-500);
    text-decoration: none;
    font-weight: 600;
    transition: color 0.15s;
  }

  .signup-link:hover {
    color: var(--orange-400);
  }

  .info-box {
    display: flex;
    gap: 12px;
    padding: var(--sp-4);
    background: rgba(223, 92, 5, 0.1);
    border: 1px solid var(--orange-500);
    border-radius: var(--radius-md);
  }

  .info-icon {
    font-size: 18px;
    flex-shrink: 0;
  }

  .info-text {
    font-size: 12px;
    color: var(--text-dim);
    line-height: 1.4;
  }

  .about-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--sp-5);
  }

  .section-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    margin-bottom: 12px;
  }

  .about-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--sp-3) 0;
    border-bottom: 1px solid var(--border);
  }

  .about-row:last-child {
    border-bottom: none;
  }

  .about-label {
    font-size: 13px;
    color: var(--text-muted);
  }

  .about-value {
    font-size: 13px;
    color: var(--text-dim);
  }

  .about-link {
    font-size: 13px;
    color: var(--orange-500);
    text-decoration: none;
    font-weight: 500;
    transition: color 0.15s;
  }

  .about-link:hover {
    color: var(--orange-400);
  }
</style>
