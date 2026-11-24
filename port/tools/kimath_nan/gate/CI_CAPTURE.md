<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# Current-image object-gate capture

The manual `object-reference` job in `rust-kimath-nan.yml` supplies the inputs that
ordinary rust-port artifacts lack. Dispatch with `object_reference=true`; all
generation inputs default false. This job is read-only and never proposes/blesses
goldens, signs a review, rebinds a lock or publishes an image.

1. The accepted `qualify.sh` seals the exact clean Linux checkout, builds the locked
   image and oracle, compares a no-cache rebuild's installed identity and runs the
   full 2M probe. It does not run the missing-golden corpus gate in this job.
2. Ninja reports the actual link command for that built oracle. `capture.py` records
   those objects in order, rejecting absent/ambiguous links, response-file commands,
   duplicate object basenames and linked/escaping inputs. It copies the actual
   compiler inputs, include/generated tree, oracle and installed identity into an
   atomic new evidence directory. This is capture, not proof or qualification.
3. The full object gate runs inside the same image under the normal Docker security
   profile, with no network, bounded container resources and a read-only sealed
   source snapshot. `--verify-closure` invokes the fixed installed-input verifier.
   Source guards run before and after the gate, including when the gate fails.
4. A genuine full PASS must emit exactly the committed generated Rust bytes. A pending
   review or any other failure stays a failed job. Logs, actual objects/includes,
   reports and stage facts are retained even on failure.

The separate frozen-reference job and original reviewed locks are unchanged. Its
whole-binary/retained-manifest constraints remain explicit. Object mode remains
unaccepted until the execution-mapping objection and context review are resolved.

Local verification before a native run: 30 binary-free tests passed (including four
capture controls with clearly dummy files), every workflow shell block passed
`bash -n`, and YAML checks confirmed read-only permissions, manual-only opt-in and
the unchanged no-golden boundary. No local image build or full object gate is
claimed by these checks; native results must be tied to their dispatched revision.
