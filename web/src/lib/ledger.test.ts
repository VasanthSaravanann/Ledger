import { describe, it, expect } from 'vitest';
import { disposeBatch, initialCandidates, isInitialScreen } from './ledger';
describe('Disposition preview rules', () => {
  it('includes only InitialScreen stage rows', () => { expect(isInitialScreen('InitialScreen')).toBe(true); expect(isInitialScreen('Interview')).toBe(false); expect(isInitialScreen('Offer')).toBe(false); });
  it('does not partially update a batch when required GATE evidence is absent', () => { expect(() => disposeBatch(initialCandidates, ['APP-0142', 'APP-0143'], 'GATE', 'GATE-02 · Required certification')).toThrow('evidence is missing'); expect(initialCandidates.find(c => c.id === 'APP-0143')?.status).toBe('Pending'); });
  it('updates all selected candidates and queues drafts', () => { const rows = disposeBatch(initialCandidates, ['APP-0142', 'APP-0143'], 'FIELD', 'FIELD-01 · Relevant experience'); expect(rows.filter(c => ['APP-0142', 'APP-0143'].includes(c.id)).map(c => [c.status, c.delivery])).toEqual([['FIELD', 'Queued'], ['FIELD', 'Queued']]); });
  it('rejects an unknown candidate before applying changes', () => { expect(() => disposeBatch(initialCandidates, ['APP-0143', 'missing'], 'ADMIN', 'ADMIN-01 · Application withdrawn')).toThrow('valid candidates'); });
});