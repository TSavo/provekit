PRAGMA foreign_keys=OFF;--> statement-breakpoint
CREATE TABLE `__new_clause_witnesses` (
	`clause_id` integer NOT NULL,
	`smt_constant` text NOT NULL,
	`model_value_id` integer NOT NULL,
	PRIMARY KEY(`clause_id`, `smt_constant`),
	FOREIGN KEY (`model_value_id`) REFERENCES `runtime_values`(`id`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`clause_id`,`smt_constant`) REFERENCES `clause_bindings`(`clause_id`,`smt_constant`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
INSERT INTO `__new_clause_witnesses`("clause_id", "smt_constant", "model_value_id") SELECT "clause_id", "smt_constant", "model_value_id" FROM `clause_witnesses`;--> statement-breakpoint
DROP TABLE `clause_witnesses`;--> statement-breakpoint
ALTER TABLE `__new_clause_witnesses` RENAME TO `clause_witnesses`;--> statement-breakpoint
PRAGMA foreign_keys=ON;--> statement-breakpoint
CREATE INDEX `cw_by_value` ON `clause_witnesses` (`model_value_id`);