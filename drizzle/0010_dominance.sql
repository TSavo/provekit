CREATE TABLE `dominance` (
	`dominator` text NOT NULL,
	`dominated` text NOT NULL,
	PRIMARY KEY(`dominator`, `dominated`),
	FOREIGN KEY (`dominator`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`dominated`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `dominance_by_dominated_dominator` ON `dominance` (`dominated`,`dominator`);--> statement-breakpoint
CREATE TABLE `post_dominance` (
	`post_dominator` text NOT NULL,
	`post_dominated` text NOT NULL,
	PRIMARY KEY(`post_dominator`, `post_dominated`),
	FOREIGN KEY (`post_dominator`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`post_dominated`) REFERENCES `nodes`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `post_dominance_by_post_dominated_post_dominator` ON `post_dominance` (`post_dominated`,`post_dominator`);
