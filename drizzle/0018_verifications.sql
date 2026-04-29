CREATE TABLE `verifications` (
	`binding_hash` text NOT NULL,
	`property_hash` text NOT NULL,
	`verdict` text NOT NULL,
	`witness` text,
	`produced_by` text NOT NULL,
	`produced_at` text NOT NULL,
	`producer_signal` text,
	PRIMARY KEY(`binding_hash`, `property_hash`, `produced_by`)
);
--> statement-breakpoint
CREATE INDEX `verifications_lookup` ON `verifications` (`binding_hash`, `property_hash`);
