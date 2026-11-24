# Implementation Task List: v1 Manual Candidate Feedback Pilot

**Scope:** Single pilot employer, single high-volume requisition (150+ applicants), initial screen stage only, 100% of rejected candidates. No engineering/ATS API integration.

---

## Phase 1: Legal, Privacy & Governance Setup
- [ ] **Task 1.1: APP 1.7–1.9 Privacy Compliance Check**
  - **Action:** Submit the v1 operational workflow to the pilot employer's legal counsel to verify privacy policy compliance against Australian Privacy Principles (APP) 1.7–1.9 ahead of the **10 December 2026** commencement date.
  - **Key Context:** Confirm that rendering a human-recorded disposition (without automated decision-making or LLM inference) meets their regulatory threshold.
- [ ] **Task 1.2: Establish Approval Workflow**
  - **Action:** Set up a single-pass sign-off process with the Hiring Manager (HM) and Legal for the pilot requisition's static copy prior to job posting/screening.
- [ ] **Task 1.3: Provision Monitored Reply Mailbox**
  - **Action:** Create or designate a real, human-monitored reply-to email address (e.g., `careers-feedback@[employer].com.au`) assigned to a named recruiter/coordinator.

---

## Phase 2: Core Artefact Creation
- [ ] **Task 2.1: Build the Single-Screen Disposition Form**
  - **Action:** Create a locked, timestamped form (e.g., Google Form, Microsoft Form, or Airtable interface) for recruiters to complete at the exact moment of bulk rejection.
  - **Constraints:**
    - Mandatory selection; **no free text** and **no "Other" option**.
    - Must capture Candidate ID/Email, Requisition ID, Recruiter ID, Timestamp, and exact Disposition Category.
    - **Allowed Options:**
      1. *Failed a stated gate* (conditional dropdown linked to the req's specific knockout questions).
      2. *Met the bar, stronger field* (comparative rejection).
      3. *Role change / internal hire / req withdrawn* (administrative closure).
- [ ] **Task 2.2: Draft the Gate-Text Library (Tier 0 Deterministic Gates)**
  - **Action:** Write exact, pre-approved copy blocks for the five standard Australian knockout criteria:
    1. Right to work
    2. Visa type / restrictions
    3. Mandatory registration / accreditation
    4. Driver's licence / trade licence
    5. Location / geographic eligibility
  - **Structure per block:** State which requirement wasn't met (based on candidate's own application answer), what would satisfy it, and whether it is waivable or time-limited.
- [ ] **Task 2.3: Draft the Role-Level Comparative Paragraph (Tier 2)**
  - **Action:** Partner with the Hiring Manager to write **one** static paragraph per requisition describing *"what a competitive application for this role looked like"* (the 2–3 attributes demonstrated by the shortlisted field).
  - **Constraints:** Zero individual adverse-action statements; identical text sent to all candidates in the "Met the bar, stronger field" bucket. Obtain HM + Legal sign-off once before opening the req.
- [ ] **Task 2.4: Create the Three Master Email Templates**
  - **Action:** Draft and approve the three merge templates corresponding to the three disposition options, ensuring the reply-to points to the named human inbox and includes zero per-candidate tailoring.

---

## Phase 3: Manual Dispatch & Logging Pipeline
- [ ] **Task 3.1: Configure ATS CSV Export & Merge Mapping**
  - **Action:** Define the manual export schema from the ATS (Candidate Name, Email, Application ID, Knockout Question Responses) and map it to the Disposition Form records via Application ID.
- [ ] **Task 3.2: Build the Immutable Send Log**
  - **Action:** Create a write-only / locked spreadsheet or database table that records every dispatched email alongside:
    - Timestamp of disposition capture
    - Timestamp of email send
    - Exact disposition selection
    - Exact text version rendered and sent
- [ ] **Task 3.3: Establish Gate-Blocking Protocol**
  - **Action:** Enforce an operational rule: no rejection email may be merged or dispatched if a candidate lacks a completed, timestamped disposition record. Log every instance where a send is blocked due to a missing disposition.

---

## Phase 4: Pilot Execution & Instrumentation
- [ ] **Task 4.1: Execute Pilot on Target Requisition**
  - **Action:** Run 100% of initial-screen rejections for the 150+ applicant requisition through the disposition form, CSV merge, and immutable log.
- [ ] **Task 4.2: Instrument & Track the Three v2 Decision Metrics**
  - **Action:** Set up tracking counters for the three empirical metrics that determine v2 viability:
    1. **Reply Volume Rate:** Number of candidate replies received per 1,000 sends (measures operational load / support burden).
    2. **Form-Block Rate:** Proportion of rejection attempts blocked because the recruiter had no recorded disposition reason (measures compliance friction).
    3. **Criteria Validation Feasibility:** Qualitative/binary tracking of whether the Hiring Manager and Legal could actually agree on and approve a structured criteria list for the role (resolves the v2 Tier 1 fork).
- [ ] **Task 4.3: Post-Pilot Go/No-Go Review**
  - **Action:** Compile the three metrics, review reply inbox themes with Legal/HR, and decide whether to proceed to v2 engineering or criteria-validation consulting.