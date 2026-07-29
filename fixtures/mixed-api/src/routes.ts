const router = require('express').Router();
router.get('/users/:id', (req, res) => res.json({id:req.params.id}));
