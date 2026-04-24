CREATE TABLE `data_flow` (
	`to_node` text NOT NULL,
	`from_node` text NOT NULL,
	`slot` text NOT NULL,
	PRIMARY KEY(`to_node`, `from_node`, `slot`),
	FOREIGN KEY (`to_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`from_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `data_flow_by_to_node_slot_from_node` ON `data_flow` (`to_node`,`slot`,`from_node`);--> statement-breakpoint
CREATE INDEX `data_flow_by_from_node_to_node` ON `data_flow` (`from_node`,`to_node`);--> statement-breakpoint
CREATE TABLE `data_flow_transitive` (
	`to_node` text NOT NULL,
	`from_node` text NOT NULL,
	PRIMARY KEY(`to_node`, `from_node`),
	FOREIGN KEY (`to_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`from_node`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `data_flow_transitive_by_from_node_to_node` ON `data_flow_transitive` (`from_node`,`to_node`);