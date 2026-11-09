# Elephants, Goldfish and the New Golden Age of Software Engineering

https://drensin.medium.com/elephants-goldfish-and-the-new-golden-age-of-software-engineering-c33641a48874


## Let’s Buy a Car!

Suppose you wanted to buy a new car. How would you approach that problem?

Most of us (including me not so long ago) would think hard about the things that seemed important to us about this car, open our favorite LLM tool in research mode, and tell it:

“I want to buy a new car. Here’s everything I’ve thought of about the car I want to buy. <INSERT WHATEVER YOU THOUGHT OF HERE> Go scour the Internet and produce the world’s most comprehensive and impressive report on which cars I should consider and which I should buy. Take as long as you want.”

Twenty minutes later we’d have a very impressive looking report that made a sensible sounding recommendation. We would nod approvingly to ourselves about how clever we were and start browsing the Internet for the best deals on our new AI-approved car choice.

Also, we did it wrong.

We used a very powerful tool like a toy and got a toy result. So.. How should we use AI to buy a car?

## Step 1: Victory Loves Preparation

Our hypothetical quest to buy the right car was destined to fail because we didn’t adequately prepare the machine to succeed on our behalf. We asked it to act as our researcher rather than our interrogator.

Here’s the prompt we should have given our beleaguered AI assistant:

“I want to buy a new car. Since this will cost me a lot of money it’s very important that I make an informed and well reasoned decision. I have thought about my needs for this new car but I am certain that I am missing critical aspects. I will tell you what I’ve come up with so far and your job is to keep asking me clarifying questions until I tell you to stop. Our job together is to explore all of the important edges of this decision. Please continue to question my assumptions and force me to clarify important points until I tell you we are done. Here is what I’ve come up with so far: <INSERT EARLIER PARAGRAPH HERE>”

Spend the next 15–20 minutes answering these questions and challenges from the AI. Occasionally ask the model “what do you think?” When you disagree, argue with it. Just be sure to argue to learn, not to win.

NOTE: If the model starts to say unhelpful things like “That’s a great point! You’re a genius!’ then it’s fallen into its sycophantic spiral. (An unavoidable consequence of all these models being told to be “friendly and helpful.”) You can usually break the model out of this loop by reminding it what it means to be actually helpful:

“When you agree with me you are not being helpful. You are most helpful when you challenge my thinking and force me to think about the edges of my problem”

This is my go-to response whenever the model starts becoming too agreeable.

After 15–20 minutes you will have a much fuller description of the ideal car you want to buy. Your initial paragraph will have expanded into 2–3 pages. If you stopped here and gave this new description to the AI you will already get a much improved result. But, we can do even better.

You are now ready for step 2.

## Step 2: Trust But Verify

A thorough description of the problem is only the first half of the recipe to get a great answer. The next step is to make sure that what the model tells you is both useful and verifiably true.

Continuing your current chat session, give it this prompt:

“Thank you for your questions but I think our description is now complete. I will be giving this description to a researcher with whom I have never worked before. I need to know that I can trust the quality of the report after they have produced it. I would like you to now adopt the persona of an expert skeptical fact checker. I would like to have a conversation with you about all the things that I should find in their report that would give me confidence that their work was thorough and correct. What sorts of links? Citations? Etc.. I will only get to ask them to perform this research once and I will have no opportunity to ask follow up questions, so I need to be very clear with them about how I will evaluate their work. Let’s discuss this now. I will let you know when we are done creating our acceptance criteria.”

Now you repeat the same interview process from the previous step except this time you are focusing exclusively on how you will grade the quality of the final output. As before, you should expect to spend 15–20 minutes on this step.

When you are done you should have another 2–3 pages of text that represent the grading rubric for the report. Now that you have more complete descriptions of both the question and the evaluation criteria you are ready for the final step.

## Step 3: Profit

Open a new session with your LLM and put it in some kind of deep research mode. Then tell it:

“I have a topic I need you to think deeply about. Use any tools at your disposal to answer this question thoroughly. I will give you both the research topic and the specific criteria I will use to evaluate the quality of your final report. Your goal is to produce a final output that thoroughly answers the question and easily passes all the evaluation criteria.

Here is my description of the topic I want you to research:

— — — — — — -

<INSERT TEXT FROM STEP 1>

— — — — — — -

Here are the criteria by which I will evaluate the quality of your output:

— — — — — — —

<INSERT TEXT FROM STEP 2>

— — — — — — -”

Now you can let the model run its deep research for as long as it takes and you can feel confident that the quality of the output you get will be much better than trying to do this with a one-shot prompt.

A Word About Judgment
None of our typical day jobs are to buy cars (although if that is your day job then you have a very cool job!). Whatever our job function (Sales, Marketing, Finance, etc) we were all hired to do the same thing — exercise excellent judgment in order to create more value for our business.

The company decides our value by the quality and impact of the judgment we consistently demonstrate.

If we are early in our careers our employer has wagered that we have the ability to rapidly acquire good judgment in our particular work domain. If we are later in our careers then the company has hired us to exercise the judgment we have already accumulated for its benefit and to help others acquire judgment of their own.

These models (at least at the time of this writing) are not capable of this kind of judgment — though they are excellent at generating sequences of words that superficially resemble it. Do not make the mistake of outsourcing your judgment to the machine.

Accelerating Future You
The steps in this section will help you leverage your judgment into much more productivity very quickly. But… The real value actually compounds over time..

SAVE the full description, acceptance criteria, and final output that you generated in these steps someplace that you can easily find. (A shared drive, a NotebookLM, etc). Over time you will accumulate an incredible amount of reasoning and history that you can leverage in future conversations with the model for some other problem.

Note: There is no need to store the full discussions; these three artifacts are sufficient, the rest turns out to be mostly token noise

Why do this? Now, you can talk to your past judgment or the collective wisdom of the team!

“Why did we make this decision this way? What was our thought process about X?” etc..

How much easier will it be to onboard new team members or to test future outputs against past reasoning? How much easier will it be to reconstruct the trajectory of a project, a team, or a long running program?

If you start doing this today the speedup and quality will compound rapidly into the future. Future You will be unrecognizably more productive than Current You.