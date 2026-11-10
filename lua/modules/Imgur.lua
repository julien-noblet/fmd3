function getinfo()
	MANGAINFO.URL=MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		x=CreateTXQuery(HTTP.Document)
		MANGAINFO.Title=x.XPathString('//meta[@property="og:title"]/@content')
		MANGAINFO.CoverLink=x.XPathString('//meta[@property="og:image"]/@content')
		MANGAINFO.Authors=x.XPathString('//a[@class="post-account"]')
		MANGAINFO.Summary=x.XPathString('//p[@class="post-image-description"]')
		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function taskstart()
	TASK.PageLinks.Clear()
	return true
end

function getpagenumber()
	local hash = URL:match('.*/(.+)$')
	if hash ~= nil then
		-- album
		if HTTP.GET(MODULE.RootURL .. '/ajaxalbums/getimages/' .. hash .. '/hit.json') then
			local x = CreateTXQuery(HTTP.Document)
			local v = x.XPath('json(*).data.images()')
			for i = 1, v.Count do
				local v1 = v.Get(i)
				TASK.PageLinks.Add('https://i.imgur.com/' .. x.XPathString('hash', v1) .. x.XPathString('ext', v1))
			end
			return true
		else
			return false
		end
	else
		-- single image
		if HTTP.GET(MaybeFillHost(MODULE.RootURL,URL)) then
			x=CreateTXQuery(HTTP.Document)
			x.XPathStringAll('//div[contains(@class,"post-image")]/a/img/concat("https:",@src)', TASK.PageLinks)
			return true
		else
			return false
		end
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID              = 'b747aace22834b26a54834866062b5c5'
	m.Name            = 'Imgur'
	m.RootURL         = 'https://imgur.com'
	m.OnGetInfo       = 'getinfo'
	m.OnTaskStart     = 'taskstart'
	m.OnGetPageNumber = 'getpagenumber'
end
