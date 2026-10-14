<script lang="ts">
    import { useAppStore } from '../state.svelte';
    import type { ExchangeAccount } from '../types';
    import styles from './ExchangeSettings.module.css';

    const app = useAppStore();

    $effect(() => {
        app.fetchExchangeKeys();
    });

    const EXCHANGES = ['Hyperliquid', 'Bybit', 'Bitget'];
    let form = app.exchangeFormDraft;
    let showForm = $state(false);

    // v10.1: DEX vs CEX — Hyperliquid is a DEX (wallet + private key),
    // Bitget is a CEX (API key / secret / passphrase / referred UID).
    const isDex = $derived(form.exchange === 'Hyperliquid');
    let hexError = $state('');

    $effect(() => {
        // Reset the format error whenever the exchange or secret changes.
        const _ = form.exchange;
        const __ = form.api_secret;
        hexError = '';
    });

    async function handleAdd() {
        if (!form.account_name.trim() || !form.api_key.trim() || !form.api_secret.trim()) return;
        // v10.1: DEX private keys must be hex (64 hex chars typical).
        if (isDex) {
            const key = form.api_secret.trim().replace(/^0x/, '');
            if (!/^[0-9a-fA-F]{1,128}$/.test(key)) {
                hexError = 'Private key must be hexadecimal (0x prefix optional).';
                return;
            }
        }
        await app.addExchangeKey();
        showForm = false;
    }

    async function handleDelete(id: number) {
        await app.deleteExchangeKey(id);
    }

    // ── Rotation & backup (v7.1) ──────────────────────────────────────
    let rotateSecret = $state('');
    let rotateMsg = $state('');
    let backupPassphrase = $state('');
    let backupMsg = $state('');

    async function handleRotate() {
        if (!rotateSecret.trim()) return;
        rotateMsg = await app.rotateExchangeKeys(rotateSecret);
        rotateSecret = '';
    }

    async function handleBackup() {
        if (!backupPassphrase.trim()) return;
        const result = await app.backupExchangeKeys(backupPassphrase);
        if (result.ok) {
            const blob = new Blob([JSON.stringify(result.json, null, 2)], { type: 'application/json' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = `exchange-keys-backup-${new Date().toISOString().slice(0, 10)}.json`;
            a.click();
            URL.revokeObjectURL(url);
            backupMsg = 'Backup downloaded — keep it in a safe place.';
        } else {
            backupMsg = result.error ?? 'Backup failed';
        }
        backupPassphrase = '';
    }

    function formatTs(ts: number | null): string {
        if (!ts) return '--';
        return new Date(ts * 1000).toLocaleString();
    }
</script>

<div class={styles.esLayout}>
    <div class={styles.esMain}>
        <!-- Add Account Form -->
        <div class={styles.esCard}>
            <div class={styles.esCardHeader}>
                <h3 class={styles.esCardTitle}>LINK NEW EXCHANGE ACCOUNT</h3>
                <button class={styles.esToggleFormBtn} onclick={() => showForm = !showForm}>
                    {showForm ? 'Cancel' : '+ Add Account'}
                </button>
            </div>

            {#if showForm}
                <div class={styles.esForm}>
                    <div class={styles.esFieldRow}>
                        <label class={styles.esLabel} for="es-exchange">Exchange</label>
                        <select id="es-exchange" class={styles.esSelect} bind:value={form.exchange}>
                            {#each EXCHANGES as ex}
                                <option value={ex}>{ex}</option>
                            {/each}
                        </select>
                    </div>
                    <div class={styles.esFieldRow}>
                        <label class={styles.esLabel} for="es-account">Account Name</label>
                        <input id="es-account" type="text" class={styles.esInput} bind:value={form.account_name} placeholder="My Account" />
                    </div>
                    {#if isDex}
                        <p class={styles.esMsg} style="color:#f59e0b; margin:0 0 8px">
                            Hyperliquid is a DEX — link a wallet (address + private key), not exchange API credentials.
                        </p>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-wallet">Wallet Address</label>
                            <input id="es-wallet" type="text" class={styles.esInput} bind:value={form.api_key} placeholder="0x…" />
                        </div>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-privkey">Private Key (hex)</label>
                            <input id="es-privkey" type="password" class={styles.esInput} bind:value={form.api_secret} placeholder="0x + 64 hex characters" />
                        </div>
                    {:else}
                        <p class={styles.esMsg} style="color:#f59e0b; margin:0 0 8px">
                            Bitget is a CEX — API key + secret + passphrase (create a trading-only key in your Bitget account).
                        </p>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-apikey">API Key</label>
                            <input id="es-apikey" type="password" class={styles.esInput} bind:value={form.api_key} placeholder="bg_…" />
                        </div>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-apisecret">API Secret</label>
                            <input id="es-apisecret" type="password" class={styles.esInput} bind:value={form.api_secret} placeholder="••••••••" />
                        </div>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-passphrase">Passphrase</label>
                            <input id="es-passphrase" type="password" class={styles.esInput} bind:value={form.passphrase} placeholder="Required for Bitget" />
                        </div>
                        <div class={styles.esFieldRow}>
                            <label class={styles.esLabel} for="es-referred">Referred UID</label>
                            <input id="es-referred" type="text" class={styles.esInput} bind:value={form.referred_uid} placeholder="Optional" />
                        </div>
                    {/if}
                    {#if hexError}
                        <p class={styles.esMsg} style="color:#ef4444">{hexError}</p>
                    {/if}
                    <div class={styles.esFieldRow}>
                        <label class={styles.esLabel} for="es-active">Active Account</label>
                        <input id="es-active" type="checkbox" bind:checked={form.is_active} class={styles.esCheckbox} />
                    </div>
                    <button class={styles.esSubmitBtn} onclick={handleAdd}
                        disabled={!form.account_name.trim() || !form.api_key.trim() || !form.api_secret.trim()}>
                        ADD ACCOUNT
                    </button>
                </div>
            {/if}
        </div>

        <!-- Linked Accounts Table -->
        <div class={styles.esCard}>
            <div class={styles.esCardHeader}>
                <h3 class={styles.esCardTitle}>LINKED ACCOUNTS</h3>
                <span class={styles.esCounter}>Active Accounts ({app.exchangeActiveCount}/{app.exchangeMaxAccounts})</span>
            </div>

            {#if app.exchangeAccounts.length === 0}
                <p class={styles.esEmpty}>No linked accounts found. Add an account to get started.</p>
            {:else}
                <div class={styles.esTableWrap}>
                    <table class={styles.esTable}>
                        <thead>
                            <tr>
                                <th>Exchange</th>
                                <th>Account Name</th>
                                <th>Key / Wallet</th>
                                <th>Secret</th>
                                <th>Passphrase</th>
                                <th>Referred UID</th>
                                <th>Status</th>
                                <th>Last Sync</th>
                                <th>Action</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each app.exchangeAccounts as account (account.id)}
                                {@const dexRow = account.exchange === 'Hyperliquid'}
                                <tr>
                                    <td>{account.exchange}</td>
                                    <td>{account.account_name}</td>
                                    <td class={styles.esMonospace}>{account.api_key.substring(0, 12)}...</td>
                                    <td>••••</td>
                                    <td>{dexRow ? '—' : '••••'}</td>
                                    <td>{dexRow ? '—' : (account.referred_uid || '--')}</td>
                                    <td>
                                        <span class="{styles.esStatus} {account.is_active ? styles.esActive : styles.esInactive}">
                                            {account.is_active ? 'Active' : 'Inactive'}
                                        </span>
                                    </td>
                                    <td class={styles.esTs}>{formatTs(account.last_sync_timestamp)}</td>
                                    <td>
                                        <button class={styles.esDeleteBtn} onclick={() => handleDelete(account.id)}>Delete</button>
                                    </td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            {/if}

            {#if app.exchangeAccounts.length > 0}
                <div class={styles.esMultiBadge}>Multi-account active</div>
            {/if}
        </div>

        <!-- Rotation & Backup (v7.1) -->
        <div class={styles.esCard}>
            <div class={styles.esCardHeader}>
                <h3 class={styles.esCardTitle}>KEY ROTATION & BACKUP</h3>
            </div>
            <div class={styles.esForm}>
                <div class={styles.esFieldRow}>
                    <label class={styles.esLabel} for="es-rotate">New Master Secret</label>
                    <input id="es-rotate" type="password" class={styles.esInput}
                        bind:value={rotateSecret} placeholder="Re-encrypt all stored secrets under a new EXCHANGE_SECRET_KEY" />
                    <button class={styles.esSubmitBtn} onclick={handleRotate}
                        disabled={!rotateSecret.trim()}>ROTATE</button>
                </div>
                {#if rotateMsg}
                    <p class={styles.esMsg}>{rotateMsg}</p>
                {/if}
                <div class={styles.esFieldRow}>
                    <label class={styles.esLabel} for="es-backup">Backup Passphrase</label>
                    <input id="es-backup" type="password" class={styles.esInput}
                        bind:value={backupPassphrase} placeholder="Passphrase that unlocks the encrypted backup" />
                    <button class={styles.esSubmitBtn} onclick={handleBackup}
                        disabled={!backupPassphrase.trim()}>DOWNLOAD BACKUP</button>
                </div>
                {#if backupMsg}
                    <p class={styles.esMsg}>{backupMsg}</p>
                {/if}
            </div>
        </div>
    </div>
</div>

