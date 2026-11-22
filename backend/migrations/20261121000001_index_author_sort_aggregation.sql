-- Read author relations in issue order without scanning cover-artist relations.
-- This supports grouped author sorting once per issue instead of per collection copy.
CREATE INDEX idx_issue_persons_role_issue_person
    ON issue_persons (role, issue_id, person_id);
