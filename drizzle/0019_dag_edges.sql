ALTER TABLE `verifications` ADD `cid` text;
--> statement-breakpoint
ALTER TABLE `verifications` ADD `input_cids` text;
--> statement-breakpoint
CREATE INDEX `verifications_cid` ON `verifications` (`cid`);
