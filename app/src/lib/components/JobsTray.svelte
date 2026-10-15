<script lang="ts">
  import { api } from '../api';
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';

  const running = $derived(app.jobs.filter((j) => j.state === 'running'));
</script>

{#if running.length > 0}
  <section class="tray card" aria-label="Background jobs" aria-live="polite">
    {#each running as job (job.id)}
      <div class="job">
        <div class="row">
          <span class="kind">{job.kind}</span>
          <span class="muted msg">{job.message}</span>
          <button class="btn ghost icon" aria-label="Cancel {job.kind}" onclick={() => api.jobsCancel(job.id)}>
            <Icon name="x" size={14} />
          </button>
        </div>
        <progress max="1" value={job.progress}></progress>
      </div>
    {/each}
  </section>
{/if}

<style>
  .tray {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    width: 320px;
    padding: var(--space-2) var(--space-3);
    box-shadow: var(--shadow);
    z-index: 40;
  }
  .job + .job {
    border-top: 1px solid var(--border);
    margin-top: var(--space-2);
    padding-top: var(--space-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .kind {
    font-weight: 600;
  }
  .msg {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  progress {
    width: 100%;
    accent-color: var(--accent);
  }
</style>
