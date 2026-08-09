ALTER TABLE series_publication_events
    DROP FOREIGN KEY fk_series_publication_events_series,
    DROP FOREIGN KEY fk_series_publication_events_actor,
    MODIFY COLUMN actor_user_id INT UNSIGNED NULL,
    ADD CONSTRAINT fk_series_publication_events_series
        FOREIGN KEY (series_id) REFERENCES series(id) ON DELETE RESTRICT,
    ADD CONSTRAINT fk_series_publication_events_actor
        FOREIGN KEY (actor_user_id) REFERENCES users(id) ON DELETE SET NULL;
